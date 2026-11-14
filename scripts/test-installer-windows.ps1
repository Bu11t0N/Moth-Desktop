#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$PackageDirectory,
    [Parameter(Mandatory)][string]$Baseline031Directory,
    [Parameter(Mandatory)][string]$Baseline032Directory,
    [string]$MakeNsis="${env:ProgramFiles(x86)}\NSIS\makensis.exe",
    [switch]$DisposableVm
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Assert-DisposableVm.ps1')
Assert-DisposableWindowsVm -DisposableVm:$DisposableVm
$root=Join-Path $env:ProgramFiles 'Moth Desktop'
$registry='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop'
if ((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $root) -or (Test-Path -LiteralPath $registry)) { throw 'A clean disposable system is required.' }
$installerPath=(Resolve-Path -LiteralPath $Installer).Path
$package=(Resolve-Path -LiteralPath $PackageDirectory).Path
$work=Join-Path $env:TEMP ('Moth-nsis-acceptance-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
Add-Type -AssemblyName System.Security
$profile=Join-Path $env:LOCALAPPDATA 'org.mothvpn.desktop/profiles.dpapi'
if (-not (Test-Path -LiteralPath $profile)) {
    New-Item -ItemType Directory -Path (Split-Path $profile) -Force | Out-Null
    $id=[Guid]::NewGuid().ToString()
    $fixture=@{version=1;library=@{selected=$id;profiles=@(@{id=$id;name='Installer acceptance';host='example.com';port=443;protocol='trojan';credential='synthetic-acceptance-only';tls=@{kind='tls';server_name='example.com';fingerprint=$null;alpn=@()};transport=@{kind='tcp'};flow=$null})}}
    $plain=[Text.Encoding]::UTF8.GetBytes(($fixture | ConvertTo-Json -Depth 10 -Compress))
    [IO.File]::WriteAllBytes($profile,[Security.Cryptography.ProtectedData]::Protect($plain,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))
}
$profileHash=(Get-FileHash -LiteralPath $profile).Hash
$original=[Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($profile),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
function Check-Profiles {
    if ((Get-FileHash -LiteralPath $profile).Hash -ne $profileHash) { throw 'Encrypted profiles changed.' }
    $plain=[Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($profile),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
    if ([Convert]::ToBase64String($plain) -cne [Convert]::ToBase64String($original)) { throw 'Profile decryption changed.' }
    [Array]::Clear($plain,0,$plain.Length)
}
function Run-Installer([string]$Path,[bool]$Success=$true) {
    $process=Start-Process -FilePath $Path -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if (($process.ExitCode -eq 0) -ne $Success) {
        foreach ($directory in Get-ChildItem -LiteralPath $env:ProgramFiles -Directory -Filter '*.moth-payload') {
            $errorFile=Join-Path $directory.FullName 'maintenance-error.txt'
            if (Test-Path -LiteralPath $errorFile) { Get-Content -LiteralPath $errorFile }
        }
        throw "Unexpected installer exit code: $($process.ExitCode)."
    }
    Check-Profiles
}
function Check-Installed {
    $health=(& (Join-Path $root 'moth-service.exe') --status) | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $health.version -ne '0.3.3' -or -not $health.systemTunnel) { throw 'Installed service health failed.' }
    if ((Get-ItemProperty -LiteralPath $registry).DisplayVersion -ne '0.3.3') { throw 'Uninstall registration missing.' }
    foreach ($name in @('moth-desktop.exe','Uninstall.exe','installed-files.json')) { if (-not (Test-Path -LiteralPath (Join-Path $root $name))) { throw 'Installed payload missing.' } }
    $shell=New-Object -ComObject WScript.Shell
    foreach ($directory in @([Environment]::GetFolderPath('CommonPrograms'),[Environment]::GetFolderPath('CommonDesktopDirectory'))) {
        $link=Join-Path $directory 'Moth Desktop.lnk'
        if (-not (Test-Path -LiteralPath $link) -or $shell.CreateShortcut($link).TargetPath -ine (Join-Path $root 'moth-desktop.exe')) { throw 'Application shortcut missing or redirected.' }
    }
    & (Join-Path $root 'moth-service.exe') --check-installed
    if ($LASTEXITCODE -ne 0) { throw 'Protected installation validation failed.' }
    Check-Profiles
}
function Uninstall {
    Run-Installer (Join-Path $root 'Uninstall.exe')
    if ((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $registry)) { throw 'Uninstall left service/registration.' }
    foreach ($directory in @([Environment]::GetFolderPath('CommonPrograms'),[Environment]::GetFolderPath('CommonDesktopDirectory'))) {
        if (Test-Path -LiteralPath (Join-Path $directory 'Moth Desktop.lnk')) { throw 'Uninstall left shortcut.' }
    }
}
# A separate build with a forced final health failure. No production test flag,
# service executable modification or signature bypass is added to the application.
$faultRepo=Join-Path $work 'fault-source'
New-Item -ItemType Directory -Path (Join-Path $faultRepo 'scripts/installer') -Force | Out-Null
$repo=Split-Path $PSScriptRoot
foreach ($name in @('package.json','LICENSE')) { Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $faultRepo }
foreach ($name in @('Service.ps1','build-installer-windows.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $faultRepo 'scripts') }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'installer/windows.nsi') -Destination (Join-Path $faultRepo 'scripts/installer')
$faultScript=Join-Path $faultRepo 'scripts/Service.ps1'
$source=Get-Content -LiteralPath $faultScript -Raw
$needle='if ($health.version -ne $expectedVersion)'
if (-not $source.Contains($needle)) { throw 'Rollback injection point missing.' }
$source.Replace($needle,'if ($true)') | Set-Content -LiteralPath $faultScript -Encoding utf8BOM
& (Join-Path $faultRepo 'scripts/build-installer-windows.ps1') -PackageDirectory $package -OutputDirectory (Join-Path $work 'fault-artifact') -MakeNsis $MakeNsis
$faultInstaller=Join-Path $work 'fault-artifact/Moth-0.3.3-windows-x64-setup.exe'
# Failed clean installation must unregister the newly created service and restore absence.
Run-Installer $faultInstaller $false
if ((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $root) -or (Test-Path -LiteralPath $registry)) { throw 'Failed clean installation did not restore absence.' }
Run-Installer $installerPath
Check-Installed
$guiHash=(Get-FileHash -LiteralPath (Join-Path $root 'moth-desktop.exe')).Hash
$uninstallerHash=(Get-FileHash -LiteralPath (Join-Path $root 'Uninstall.exe')).Hash
foreach ($running in @($true,$false)) {
    if (-not $running) { Stop-Service MothDesktopService }
    Run-Installer $faultInstaller $false
    $expected=if ($running) {'Running'} else {'Stopped'}
    if ((Get-Service MothDesktopService).Status -ne $expected -or (Get-FileHash -LiteralPath (Join-Path $root 'moth-desktop.exe')).Hash -ne $guiHash -or (Get-FileHash -LiteralPath (Join-Path $root 'Uninstall.exe')).Hash -ne $uninstallerHash) { throw 'Desktop rollback did not restore payload/service state.' }
    if (-not $running) { Start-Service MothDesktopService }
    Check-Installed
}
Run-Installer $installerPath
Check-Installed
Uninstall
if (Test-Path -LiteralPath $root) { throw 'Clean uninstall left unchanged application files.' }
foreach ($baseline in @($Baseline031Directory,$Baseline032Directory)) {
    & (Join-Path $baseline 'Service.ps1') -Action Install -PackageDirectory $baseline
    Run-Installer $installerPath
    Check-Installed
    Uninstall
    if (Test-Path -LiteralPath $root) { throw 'Upgraded installation did not uninstall cleanly.' }
}
# Preserve unknown and modified files; never recursively erase installation data.
Run-Installer $installerPath
$unknown=Join-Path $root 'retain.txt'
[IO.File]::WriteAllText($unknown,'Preserve unexpected content')
$modified=Join-Path $root 'README.txt'
[IO.File]::AppendAllText($modified,' Preserve changed content')
$modifiedHash=(Get-FileHash -LiteralPath $modified).Hash
Uninstall
if (-not (Test-Path -LiteralPath $unknown) -or (Get-FileHash -LiteralPath $modified).Hash -ne $modifiedHash -or (Test-Path -LiteralPath (Join-Path $root 'moth-service.exe'))) { throw 'Uninstall retention policy failed.' }
# Retain this fixture without blocking the subsequent SCM acceptance.
Move-Item -LiteralPath $root -Destination (Join-Path $env:ProgramFiles ('Moth acceptance retained-'+[Guid]::NewGuid().ToString('N')))
[Array]::Clear($original,0,$original.Length)
Write-Output 'NSIS clean install, legacy upgrades, running/stopped rollback, retry, shortcuts, uninstall and DPAPI/file retention passed.'
