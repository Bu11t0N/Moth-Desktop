#requires -Version 5.1
# Explicit destructive installation acceptance, ONLY inside a disposable VM.
param(
    [Parameter(Mandatory)][string]$CandidateDirectory,
    [Parameter(Mandatory)][string]$Baseline031Directory,
    [Parameter(Mandatory)][string]$Baseline032Directory,
    [switch]$DisposableVm
)
$ErrorActionPreference='Stop'
$computer=Get-CimInstance Win32_ComputerSystem
$ci=$env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_OS -eq 'Windows'
if (-not $ci -and (-not $DisposableVm -or $computer.Manufacturer -notmatch 'VMware' -or $computer.Model -notmatch 'VMware|Virtual')) { throw 'Explicit disposable VMware VM or Windows CI runner required.' }
$candidate=(Resolve-Path -LiteralPath $CandidateDirectory).Path
$installer=Join-Path $candidate 'Service.ps1'
$root=Join-Path $env:ProgramFiles 'Moth Desktop'
if ((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $root)) { throw 'A clean disposable VM is required; existing installations are never removed by this test.' }
$work=Join-Path $env:TEMP ('Moth-upgrade-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
# A real, encrypted profile in the application's actual per-user directory.
$profileDirectory=Join-Path $env:LOCALAPPDATA 'org.mothvpn.desktop'
$profileFile=Join-Path $profileDirectory 'profiles.dpapi'
if (Test-Path -LiteralPath $profileFile) { throw 'Existing profiles must not be overwritten by acceptance.' }
New-Item -ItemType Directory -Path $profileDirectory -Force | Out-Null
Add-Type -AssemblyName System.Security
$id=[Guid]::NewGuid().ToString()
$plain=[Text.Encoding]::UTF8.GetBytes((@{version=1;library=@{selected=$id;profiles=@(@{id=$id;name='Acceptance profile';host='example.com';port=443;protocol='trojan';credential='synthetic-acceptance-only';tls=@{kind='tls';server_name='example.com';fingerprint=$null;alpn=@()};transport=@{kind='tcp'};flow=$null})}} | ConvertTo-Json -Depth 10 -Compress))
[IO.File]::WriteAllBytes($profileFile,[Security.Cryptography.ProtectedData]::Protect($plain,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))
$profileHash=(Get-FileHash -LiteralPath $profileFile).Hash
function Check-Profiles {
    if ((Get-FileHash -LiteralPath $profileFile).Hash -ne $profileHash) { throw 'Encrypted profiles changed during service maintenance.' }
    $decrypted=[Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($profileFile),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
    if ([Text.Encoding]::UTF8.GetString($decrypted) -ne [Text.Encoding]::UTF8.GetString($plain)) { throw 'Profile decryption changed.' }
}
function Health-Version {
    $health=(& (Join-Path $candidate 'moth-service.exe') --status) | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $health.protocol -ne 2) { throw 'Authenticated health failed.' }
    return $health.version
}
function Retire-Installation {
    & $installer -Action Remove
    if (Get-Service MothDesktopService -ErrorAction SilentlyContinue) { throw 'Service remained registered.' }
    if (Test-Path -LiteralPath $root) { Move-Item -LiteralPath $root -Destination (Join-Path $env:ProgramFiles ('Moth acceptance retired-'+[Guid]::NewGuid().ToString('N'))) }
    Check-Profiles
}
# Fail the final health assertion AFTER candidate startup. Production scripts have no test override.
$fault=Join-Path $work 'Service-failing-health.ps1'
$source=Get-Content -LiteralPath $installer -Raw
$needle='if ($health.version -ne $expectedVersion)'
if (-not $source.Contains($needle)) { throw 'Rollback injection point missing.' }
$source.Replace($needle,'if ($true)') | Set-Content -LiteralPath $fault -Encoding UTF8
foreach ($baseline in @($Baseline031Directory,$Baseline032Directory)) {
    & (Join-Path $baseline 'Service.ps1') -Action Install -PackageDirectory $baseline
    $before=Health-Version
    if ($before -notin @('0.3.1','0.3.2')) { throw 'Unexpected baseline version.' }
    $oldHash=(Get-FileHash -LiteralPath (Join-Path $root 'moth-service.exe')).Hash
    $bad=Join-Path $work ('tampered-'+$before)
    New-Item -ItemType Directory -Path (Join-Path $bad 'engines') -Force | Out-Null
    foreach($relative in @('moth-service.exe','engines\xray.exe','engines\sing-box.exe')) {
        Copy-Item -LiteralPath (Join-Path $candidate $relative) -Destination (Join-Path $bad $relative)
    }
    [IO.File]::AppendAllText((Join-Path $bad 'engines\xray.exe'),'modified')
    $rejected=$false
    try { & $installer -Action Install -PackageDirectory $bad } catch { $rejected=$true }
    if (-not $rejected -or (Health-Version) -ne $before) { throw 'Modified core was accepted or disturbed the existing service.' }
    $secureAcl=Get-Acl -LiteralPath $root
    $unsafeAcl=Get-Acl -LiteralPath $root
    $unsafeAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'),'Write','ContainerInherit, ObjectInherit','None','Allow'))
    try {
        Set-Acl -LiteralPath $root -AclObject $unsafeAcl
        $rejected=$false
        try { & $installer -Action Install -PackageDirectory $candidate } catch { $rejected=$true }
        if (-not $rejected) { throw 'Unsafe existing installation ACL was accepted.' }
    } finally { Set-Acl -LiteralPath $root -AclObject $secureAcl }
    if ((Health-Version) -ne $before) { throw 'ACL refusal disturbed the existing service.' }


    $rejected=$false
    try { & $fault -Action Install -PackageDirectory $candidate } catch { $rejected=$true }
    if (-not $rejected -or (Health-Version) -ne $before -or (Get-FileHash -LiteralPath (Join-Path $root 'moth-service.exe')).Hash -ne $oldHash) { throw 'Rollback failed to restore running baseline.' }
    Check-Profiles
    Stop-Service MothDesktopService
    $rejected=$false
    try { & $fault -Action Install -PackageDirectory $candidate } catch { $rejected=$true }
    if (-not $rejected -or (Get-Service MothDesktopService).Status -ne 'Stopped' -or (Get-FileHash -LiteralPath (Join-Path $root 'moth-service.exe')).Hash -ne $oldHash) { throw 'Rollback did not preserve the original stopped service state.' }
    Start-Service MothDesktopService
    if ((Health-Version) -ne $before) { throw 'Stopped-service rollback did not restore baseline.' }
    Check-Profiles
    & $installer -Action Install -PackageDirectory $candidate
    if ((Health-Version) -ne '0.3.3') { throw 'Upgrade did not activate candidate.' }
    Check-Profiles
    # Repeated update, stopped-service update and removal/reinstall without manual cleanup.
    & $installer -Action Install -PackageDirectory $candidate
    Stop-Service MothDesktopService
    & $installer -Action Install -PackageDirectory $candidate
    & $installer -Action Remove
    & $installer -Action Install -PackageDirectory $candidate
    if ((Health-Version) -ne '0.3.3') { throw 'Reinstallation failed.' }
    Retire-Installation
    Write-Output "Upgrade from $before, post-start failure rollback, repeat/stopped update, removal/reinstall and DPAPI preservation passed."
}
# Clean installation and failed clean installation cleanup.
$rejected=$false
try { & $fault -Action Install -PackageDirectory $candidate } catch { $rejected=$true }
if (-not $rejected -or (Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $root)) { throw 'Failed clean install was not rolled back.' }
& $installer -Action Install -PackageDirectory $candidate
if ((Health-Version) -ne '0.3.3') { throw 'Clean install failed.' }
Check-Profiles
Retire-Installation
Write-Output 'Clean installation, failed clean installation cleanup and profile preservation passed.'
