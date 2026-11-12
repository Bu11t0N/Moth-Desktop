# SCM acceptance ONLY on a disposable GitHub-hosted Windows runner, never the developer host.
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'This smoke test requires a disposable GitHub Actions Windows runner.' }
$repo = Split-Path $PSScriptRoot
$package = Join-Path $repo 'target\release'
$installed = $false
try {
    & (Join-Path $PSScriptRoot 'Service.ps1') -Action Install -PackageDirectory $package
    $installed = $true
    if ((Get-Service MothDesktopService).Status -ne 'Running') { throw 'Service did not reach RUNNING' }
    Stop-Service MothDesktopService
    (Get-Service MothDesktopService).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(15))
    Start-Service MothDesktopService
    (Get-Service MothDesktopService).WaitForStatus('Running',[TimeSpan]::FromSeconds(20))
    & (Join-Path $env:ProgramFiles 'Moth Desktop\moth-service.exe') --status
    if ($LASTEXITCODE -ne 0) { throw 'Service did not recover after stop/start' }
} finally {
    if ($installed) { & (Join-Path $PSScriptRoot 'Service.ps1') -Action Remove }
}
