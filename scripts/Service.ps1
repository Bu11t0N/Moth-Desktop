#requires -Version 5.1
param(
    [ValidateSet('Install','Remove','Status')][string]$Action = 'Status',
    [string]$PackageDirectory = ''
)
$ErrorActionPreference = 'Stop'
$serviceName = 'MothDesktopService'
$installRoot = Join-Path $env:ProgramFiles 'Moth Desktop'
$expectedVersion = '0.3.3'
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
if ($Action -eq 'Status') {
    Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    return
}
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run this explicit installation/removal action in an elevated PowerShell window.' }
Assert-PlainTree $installRoot
$service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
$serviceExe = Join-Path $installRoot 'moth-service.exe'
if ($service) {
    $configuration = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($configuration.PathName -cne ('"' + $serviceExe + '" --service') -or $configuration.StartName -ne 'LocalSystem') { throw 'Unexpected service configuration; installation was not changed.' }
}
if ($Action -eq 'Remove') {
    Stop-Moth
    if ($service) {
        & "$env:SystemRoot\System32\sc.exe" delete $serviceName | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Service deletion failed' }
    }
    Write-Output 'Service removed. Installation files and user profiles were preserved; Install can reuse this directory.'
    return
}
if (-not $PackageDirectory) { throw 'Specify the fully extracted package directory.' }
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
Assert-PlainTree $package
$client = Join-Path $package 'moth-service.exe'
$hashes = @{
    'engines\xray.exe' = 'c77a00eb84fb93ae044c2d771a1cabcc033b81e487253fbc35b12df6879c1e15'
    'engines\sing-box.exe' = 'fc7b65219abe8a0166d0b4891a2f7cabcbcc13b3adcf89e6d5913743a67aba10'
}
if (-not (Test-Path -LiteralPath $client -PathType Leaf)) { throw 'Package service executable missing.' }
$version = & $client --version
if ($LASTEXITCODE -ne 0 -or $version -ne $expectedVersion) { throw 'Package service version does not match this installer.' }
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
        if ($oldHealth.version -notin @('0.3.1','0.3.2','0.3.3')) { throw 'Existing service version is outside the supported upgrade range.' }
        $tunnel = (& $client --tunnel-status) | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0 -or $tunnel.kind -ne 'tunnel' -or $tunnel.phase -notin @('disconnected','failed')) { throw 'Disconnect VPN before upgrading the service (including other accounts).' }
    } finally {
        if (-not $wasRunning) { Stop-Moth }
    }
}
$stage = Join-Path $env:ProgramFiles ('Moth Desktop.stage-' + [Guid]::NewGuid().ToString('N'))
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
