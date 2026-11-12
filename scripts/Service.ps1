#requires -Version 7.0
param(
    [ValidateSet('Install','Remove','Status')][string]$Action = 'Status',
    [string]$PackageDirectory = ''
)
$ErrorActionPreference = 'Stop'
$serviceName = 'MothDesktopService'
$installRoot = Join-Path $env:ProgramFiles 'Moth Desktop'
if ($Action -eq 'Status') {
    Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    return
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this explicit installation/removal action in an elevated PowerShell 7 window.'
}
if ($Action -eq 'Remove') {
    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    if ($service) {
        Stop-Service -Name $serviceName
        $service.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(15))
        & "$env:SystemRoot\System32\sc.exe" delete $serviceName
        if ($LASTEXITCODE -ne 0) { throw 'Service deletion failed' }
    }
    Write-Output 'Service removed. Installation files were preserved.'
    return
}
if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) { throw 'Service already exists; this preview does not implement in-place upgrades.' }
if (Test-Path -LiteralPath $installRoot) { throw 'Installation directory already exists. Review it before installing this development preview.' }
if (-not $PackageDirectory) { throw 'Specify the directory of the fully extracted preview package.' }
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
foreach ($relative in @('moth-service.exe','engines\xray.exe','engines\sing-box.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $package $relative) -PathType Leaf)) { throw "Package component missing: $relative" }
}
New-Item -ItemType Directory -Path $installRoot | Out-Null
$acl = [Security.AccessControl.DirectorySecurity]::new()
$admins = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
$acl.SetOwner($admins)
$acl.SetAccessRuleProtection($true,$false)
foreach ($sid in @('S-1-5-18','S-1-5-32-544','S-1-5-32-545')) {
    $rights = if ($sid -eq 'S-1-5-32-545') { 'ReadAndExecute, Synchronize' } else { 'FullControl' }
    $rule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),$rights,'ContainerInherit, ObjectInherit','None','Allow')
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $installRoot -AclObject $acl
Copy-Item -LiteralPath (Join-Path $package 'moth-service.exe') -Destination $installRoot
New-Item -ItemType Directory -Path (Join-Path $installRoot 'engines') | Out-Null
foreach ($core in @('xray.exe','sing-box.exe')) {
    Copy-Item -LiteralPath (Join-Path $package "engines\$core") -Destination (Join-Path $installRoot 'engines')
}
if (Test-Path -LiteralPath (Join-Path $package 'notices')) { Copy-Item -LiteralPath (Join-Path $package 'notices') -Destination $installRoot -Recurse }
& "$env:SystemRoot\System32\icacls.exe" $installRoot /setowner '*S-1-5-32-544' /T /C | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not set installation ownership' }
$serviceExe = Join-Path $installRoot 'moth-service.exe'
& $serviceExe --check-install
if ($LASTEXITCODE -ne 0) { throw 'Service rejected the installation security policy; no service was registered.' }
New-Service -Name $serviceName -DisplayName 'Moth Desktop Service (development)' -BinaryPathName ('"' + $serviceExe + '" --service') -StartupType Manual -Description 'Moth local server diagnostics. System VPN is not implemented in this preview.' | Out-Null
try {
    Start-Service -Name $serviceName
    (Get-Service -Name $serviceName).WaitForStatus('Running',[TimeSpan]::FromSeconds(20))
    & $serviceExe --status
    if ($LASTEXITCODE -ne 0) { throw 'Service authentication/health check failed' }
} catch {
    Stop-Service -Name $serviceName -ErrorAction SilentlyContinue
    & "$env:SystemRoot\System32\sc.exe" delete $serviceName | Out-Null
    throw
}
