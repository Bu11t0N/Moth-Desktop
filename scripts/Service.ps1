#requires -Version 5.1
param(
    [ValidateSet('Install','Remove','Status')][string]$Action = 'Status',
    [string]$PackageDirectory = '',
    [switch]$InstallDesktop
)
$ErrorActionPreference = 'Stop'
# Do not inherit caller-controlled module/search/install roots into elevated
# maintenance. This also prevents PowerShell 7 module paths breaking Windows
# PowerShell 5.1 when the graphical installer is launched from a pwsh session.
$env:SystemRoot = [Environment]::GetFolderPath('Windows')
$env:PSModulePath = [IO.Path]::Combine($PSHOME,'Modules') + [IO.Path]::PathSeparator + [IO.Path]::Combine($env:SystemRoot,'System32\WindowsPowerShell\v1.0\Modules')
$env:ProgramFiles = [Environment]::GetFolderPath('ProgramFiles')
$env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
$env:PATHEXT = '.COM;.EXE;.BAT;.CMD'
$env:ComSpec = [IO.Path]::Combine($env:SystemRoot,'System32\cmd.exe')
if ([IntPtr]::Size -ne 8) { throw '64-bit PowerShell is required for Moth Desktop maintenance.' }
$serviceName = 'MothDesktopService'
$installRoot = Join-Path $env:ProgramFiles 'Moth Desktop'
$expectedVersion = '0.3.6'
trap {
    # The graphical installer uses a protected temporary payload directory.
    # Keep a bounded, credential-free reason there when its silent run fails.
    $failure = $_
    try {
        if ($InstallDesktop -and $PackageDirectory -and (Test-Path -LiteralPath $PackageDirectory -PathType Container)) {
            $item = Get-Item -LiteralPath $PackageDirectory -Force
            if ($item.Parent.FullName -ieq $env:ProgramFiles -and $item.Name.EndsWith('.moth-payload') -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                $bytes = [Text.Encoding]::UTF8.GetBytes($failure.Exception.Message.Substring(0,[Math]::Min(2048,$failure.Exception.Message.Length)))
                $log = [IO.File]::Open((Join-Path $PackageDirectory 'maintenance-error.txt'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
                try { $log.Write($bytes,0,$bytes.Length) } finally { $log.Dispose() }
            }
        }
    } catch { } # Never overwrite a pre-existing diagnostic file or hide the real failure.
    throw $failure
}
function Assert-PlainTree([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse points are not allowed in installation paths.' }
    if ($item.PSIsContainer) {
        foreach ($child in Get-ChildItem -LiteralPath $Path -Force) { Assert-PlainTree $child.FullName }
    }
}
function Protect-Directory([string]$Path) {
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $admins = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
    $acl.SetOwner($admins)
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($sid in @('S-1-5-18','S-1-5-32-544','S-1-5-32-545')) {
        $rights = if ($sid -eq 'S-1-5-32-545') { 'ReadAndExecute, Synchronize' } else { 'FullControl' }
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),$rights,'ContainerInherit, ObjectInherit','None','Allow'))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}
function Read-Health([string]$Client) {
    $json = & $Client --status
    if ($LASTEXITCODE -ne 0) { throw 'Authenticated service health check failed.' }
    $health = $json | ConvertFrom-Json
    if ($health.protocol -ne 2 -or -not $health.systemTunnel -or @($health.engines).Count -ne 2 -or @($health.engines | Where-Object { -not $_.available }).Count) {
        throw 'Service protocol or engine health is incompatible.'
    }
    return $health
}
function Stop-Moth {
    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    if ($service -and $service.Status -ne 'Stopped') {
        Stop-Service -Name $serviceName
        $service.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))
    }
}
function Start-Moth {
    Start-Service -Name $serviceName
    (Get-Service -Name $serviceName).WaitForStatus('Running',[TimeSpan]::FromSeconds(20))
}
function Assert-DesktopClosed {
    foreach ($process in Get-Process -Name moth-desktop -ErrorAction SilentlyContinue) {
        if ($process.Path -and [IO.Path]::GetFullPath($process.Path) -ieq (Join-Path $installRoot 'moth-desktop.exe')) {
            throw 'Exit Moth Desktop from its tray menu before installing or removing it.'
        }
    }
}
function Read-Payload([string]$Directory, [string]$Name) {
    $manifestPath = Join-Path $Directory $Name
    if ((Get-Item -LiteralPath $manifestPath).Length -gt 262144) { throw 'Package manifest is too large.' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.version -ne $expectedVersion -or @($manifest.files).Count -gt 256) { throw 'Invalid desktop package manifest.' }
    $seen = @{}
    foreach ($entry in $manifest.files) {
        $relative = [string]$entry.path
        # Fixed executable names and notice files only; no arbitrary destination paths.
        if ($relative -notmatch '^(moth-desktop\.exe|moth-service\.exe|moth-maintenance\.exe|Service\.ps1|Uninstall\.exe|README\.txt|engines/((xray|sing-box|mihomo)\.exe|Geo(IP|Site)\.dat)|notices/[A-Za-z0-9_./-]+)$' -or
            $relative.Contains('..') -or $entry.sha256 -notmatch '^[a-f0-9]{64}$' -or $seen.ContainsKey($relative)) { throw 'Invalid desktop package entry.' }
        $seen[$relative] = $true
    }
    foreach ($required in @('moth-desktop.exe','moth-service.exe','moth-maintenance.exe','Service.ps1','engines/xray.exe','engines/sing-box.exe','engines/mihomo.exe','engines/GeoIP.dat','engines/GeoSite.dat')) {
        if (-not $seen.ContainsKey($required)) { throw 'Desktop package is incomplete.' }
    }
    return $manifest
}
if ($Action -eq 'Status') {
    Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    return
}
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run this explicit installation/removal action in an elevated PowerShell window.' }
$installationMutex = [Threading.Mutex]::new($false,'Global\MothDesktopPackageTransaction')
$ownsMutex = $false
try { $ownsMutex = $installationMutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $ownsMutex = $true }
if (-not $ownsMutex) { $installationMutex.Dispose(); throw 'Another Moth installation or removal is already running.' }
try {
Assert-PlainTree $installRoot
if ($InstallDesktop) { Assert-DesktopClosed }
$service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
$serviceExe = Join-Path $installRoot 'moth-service.exe'
if ($service) {
    $configuration = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($configuration.PathName -cne ('"' + $serviceExe + '" --service') -or $configuration.StartName -ne 'LocalSystem') { throw 'Unexpected service configuration; installation was not changed.' }
}
if ($Action -eq 'Remove') {
    $installed = $null
    if ($InstallDesktop) { $installed = Read-Payload $installRoot 'installed-files.json' }
    Stop-Moth
    # Explicit removal releases only Moth's persistent filters. An upgrade or
    # ordinary service stop must never silently clear the chosen protection.
    if ($service -and (Test-Path -LiteralPath $serviceExe)) {
        $guardServiceVersion = & $serviceExe --version
        if ($LASTEXITCODE -ne 0) { throw 'Could not determine installed service version.' }
        if ([Version]$guardServiceVersion -ge [Version]'0.3.5') {
            & $serviceExe --release-guard
            if ($LASTEXITCODE -ne 0) { throw 'Could not release Moth egress guard; installation preserved for administrator recovery.' }
        } elseif (Test-Path -LiteralPath (Join-Path $env:ProgramData 'Moth Desktop/egress-guard-v1.json')) {
            throw 'A newer Moth egress guard is present. Use the 0.3.5 administrator recovery tool before removing this older installation.'
        }
    }
    if ($service) {
        & "$env:SystemRoot\System32\sc.exe" delete $serviceName | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Service deletion failed' }
    }
    if ($installed) {
        # Remove only unchanged files recorded by this installer. Unknown/modified files stay.
        foreach ($entry in $installed.files) {
            $path = Join-Path $installRoot $entry.path
            if ((Test-Path -LiteralPath $path -PathType Leaf) -and (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $entry.sha256) {
                Remove-Item -LiteralPath $path
            }
        }
        Remove-Item -LiteralPath (Join-Path $installRoot 'installed-files.json')
        foreach ($directory in Get-ChildItem -LiteralPath $installRoot -Directory -Recurse | Sort-Object { $_.FullName.Length } -Descending) {
            if (-not (Get-ChildItem -LiteralPath $directory.FullName -Force)) { Remove-Item -LiteralPath $directory.FullName }
        }
        if (-not (Get-ChildItem -LiteralPath $installRoot -Force)) { Remove-Item -LiteralPath $installRoot }
        Write-Output 'Application and service removed. User profiles, modified files and previous installation backups were preserved.'
    } else { Write-Output 'Service removed. Installation files and user profiles were preserved; Install can reuse this directory.' }
    return
}
if (-not $PackageDirectory) { throw 'Specify the fully extracted package directory.' }
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
Assert-PlainTree $package
$desktopPayload = $null
if ($InstallDesktop) {
    $desktopPayload = Read-Payload $package 'desktop-package.json'
    foreach ($entry in $desktopPayload.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $package $entry.path) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw 'Desktop package checksum mismatch.' }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $package 'Uninstall.exe') -PathType Leaf)) { throw 'Desktop uninstaller missing.' }
} elseif (Test-Path -LiteralPath (Join-Path $installRoot 'installed-files.json')) {
    throw 'Use the graphical installer to update an installed desktop application.'
}
$client = Join-Path $package 'moth-service.exe'
$hashes = @{
    'engines\xray.exe' = 'c77a00eb84fb93ae044c2d771a1cabcc033b81e487253fbc35b12df6879c1e15'
    'engines\sing-box.exe' = 'fc7b65219abe8a0166d0b4891a2f7cabcbcc13b3adcf89e6d5913743a67aba10'
    'engines\mihomo.exe' = 'a4fbf2b8bc2b5416ce2d0d392ac59f2e0a234ea4fcf635b66bd21e62ea9641b9'
    'engines\GeoIP.dat' = 'eedb24b3b9fcba27e06b92c0428dab90d9496083091ae4cf39e11387fd99d0e2'
    'engines\GeoSite.dat' = 'a033da583af17f15c9743f0b29aa5c073e9c1b8018a99ab4e69bc9ed5248a086'
}
if (-not (Test-Path -LiteralPath $client -PathType Leaf)) { throw 'Package service executable missing.' }
$version = & $client --version
if ($LASTEXITCODE -ne 0 -or $version -ne $expectedVersion) { throw "Package service version does not match this installer (exit=$LASTEXITCODE; reported=$version; expected=$expectedVersion)." }
foreach ($relative in $hashes.Keys) {
    if ((Get-FileHash -LiteralPath (Join-Path $package $relative) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hashes[$relative]) { throw 'Package engine checksum mismatch.' }
}
$wasRunning = $service -and $service.Status -eq 'Running'
if ($service) {
    & $client --check-installed
    if ($LASTEXITCODE -ne 0) { throw 'Existing installation security validation failed; service was not changed.' }
    # Authenticate the existing LocalSystem endpoint with the NEW client, never execute old writable files.
    try {
        if (-not $wasRunning) { Start-Moth }
        $oldHealth = Read-Health $client
        $oldVersion = $null
        if (-not [Version]::TryParse($oldHealth.version,[ref]$oldVersion) -or $oldVersion -lt [Version]'0.3.1' -or $oldVersion -gt [Version]$expectedVersion) { throw 'Existing service version is outside the supported upgrade range.' }
        $statusArgument=if($oldVersion -ge [Version]'0.3.5'){'--connection-status'}else{'--tunnel-status'}
        $statusKind=if($oldVersion -ge [Version]'0.3.5'){'connection'}else{'tunnel'}
        $tunnel = (& $client $statusArgument) | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0 -or $tunnel.kind -ne $statusKind -or $tunnel.phase -notin @('disconnected','failed')) { throw 'Disconnect the active VPN/proxy before upgrading the service (including other accounts).' }
    } finally {
        if (-not $wasRunning) { Stop-Moth }
    }
}
$stage = Join-Path $env:ProgramFiles ('Moth Desktop.stage-' + [Guid]::NewGuid().ToString('N'))
& $client --prepare-install
if ($LASTEXITCODE -ne 0) { throw 'Protected maintenance state initialization failed; existing service was not changed.' }
$backup = Join-Path $env:ProgramFiles ('Moth Desktop.rollback-' + [Guid]::NewGuid().ToString('N'))
$hadRoot = Test-Path -LiteralPath $installRoot
$registered = $false
$movedOld = $false
$promoted = $false
$stopping = $false
try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    Protect-Directory $stage
    Copy-Item -LiteralPath $client -Destination $stage
    New-Item -ItemType Directory -Path (Join-Path $stage 'engines') | Out-Null
    foreach ($relative in $hashes.Keys) {
        Copy-Item -LiteralPath (Join-Path $package $relative) -Destination (Join-Path $stage $relative)
        if ((Get-FileHash -LiteralPath (Join-Path $stage $relative) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hashes[$relative]) { throw 'Staged engine checksum mismatch.' }
    }
    if ((Get-FileHash -LiteralPath (Join-Path $stage 'moth-service.exe')).Hash -ne (Get-FileHash -LiteralPath $client).Hash) { throw 'Staged service checksum mismatch.' }
    if (Test-Path -LiteralPath (Join-Path $package 'notices')) { Copy-Item -LiteralPath (Join-Path $package 'notices') -Destination $stage -Recurse }
    if ($desktopPayload) {
        foreach ($entry in $desktopPayload.files) {
            $destination = Join-Path $stage $entry.path
            New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $package $entry.path) -Destination $destination -Force
            if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw 'Staged desktop checksum mismatch.' }
        }
        Copy-Item -LiteralPath (Join-Path $package 'Uninstall.exe') -Destination $stage -Force
        $uninstallerHash = (Get-FileHash -LiteralPath (Join-Path $stage 'Uninstall.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
        $files = @($desktopPayload.files | Where-Object { $_.path -ne 'Uninstall.exe' }) + @(@{path='Uninstall.exe';sha256=$uninstallerHash})
        @{version=$expectedVersion;files=$files} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $stage 'installed-files.json') -Encoding UTF8
    }
    & "$env:SystemRoot\System32\icacls.exe" $stage /setowner '*S-1-5-32-544' /T /C | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not set installation ownership.' }
    $stopping = $true
    Stop-Moth
    if ($hadRoot) { Move-Item -LiteralPath $installRoot -Destination $backup; $movedOld = $true }
    Move-Item -LiteralPath $stage -Destination $installRoot
    $promoted = $true
    & $serviceExe --check-install
    if ($LASTEXITCODE -ne 0) { throw 'Installation security validation failed.' }
    if (-not $service) {
        New-Service -Name $serviceName -DisplayName 'Moth Desktop Service' -BinaryPathName ('"' + $serviceExe + '" --service') -StartupType Automatic -Description 'Moth VPN tunnel and authenticated local control.' | Out-Null
        $registered = $true
    }
    Start-Moth
    $health = Read-Health $client
    if ($health.version -ne $expectedVersion) { throw 'Started service version mismatch.' }
    if ($InstallDesktop) {
        $updater = Start-Process -FilePath (Join-Path $installRoot 'moth-maintenance.exe') -ArgumentList '--install' -WindowStyle Hidden -Wait -PassThru
        if ($updater.ExitCode -ne 0) { throw 'Protected updater installation failed.' }
    }
    Write-Output 'Service installation/update verified. User profiles were not modified.'
    if ($movedOld) { Write-Output "Previous installation retained: $backup" }
} catch {
    $failure = $_
    try {
        # Only stop after promotion: a staging failure must leave the old running service untouched.
        if ($promoted -or $movedOld) { Stop-Moth }
        if ($registered) {
            & "$env:SystemRoot\System32\sc.exe" delete $serviceName | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'Could not remove failed service registration.' }
        }
        if ($promoted) {
            # Preserve failed installation for review; never recursively delete user content.
            Move-Item -LiteralPath $installRoot -Destination ($stage + '.failed')
        }
        if ($movedOld) { Move-Item -LiteralPath $backup -Destination $installRoot }
        if ($service -and $wasRunning -and $stopping) {
            Start-Moth
            $restored = Read-Health $client
            if ($restored.version -ne $oldHealth.version) { throw 'Restored service version mismatch.' }
        }
    } catch { throw 'Update failed and automatic recovery could not be verified. Previous files were preserved. Keep VPN disconnected and review the service installation.' }
    throw $failure
}
} finally {
    if ($ownsMutex) { $installationMutex.ReleaseMutex() }
    $installationMutex.Dispose()
}
