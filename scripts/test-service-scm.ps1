# SCM acceptance ONLY on a disposable GitHub-hosted Windows runner, never the developer host.
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'This smoke test requires a disposable GitHub Actions Windows runner.' }
$repo = Split-Path $PSScriptRoot
$package = Join-Path $repo 'target\release'
$installed = $false
$testUser = 'MothCiProbe'
$createdUser = $false
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
    if (Get-LocalUser -Name $testUser -ErrorAction SilentlyContinue) { throw 'Test user already exists' }
    $password = ConvertTo-SecureString ([Guid]::NewGuid().ToString('N') + 'aA1!') -AsPlainText -Force
    New-LocalUser -Name $testUser -Password $password -AccountNeverExpires -PasswordNeverExpires | Out-Null
    $createdUser = $true
    $testSid = (Get-LocalUser -Name $testUser).SID.Value
    if (-not (Get-LocalGroupMember -SID 'S-1-5-32-545' | Where-Object { $_.SID.Value -eq $testSid })) {
        Add-LocalGroupMember -SID 'S-1-5-32-545' -Member "$env:COMPUTERNAME\$testUser"
    }
    $credential = [PSCredential]::new("$env:COMPUTERNAME\$testUser",$password)
    $userOut = Join-Path $env:RUNNER_TEMP 'moth-standard-user-health.json'
    $userErr = Join-Path $env:RUNNER_TEMP 'moth-standard-user-error.txt'
    $process = Start-Process -FilePath (Join-Path $env:ProgramFiles 'Moth Desktop\moth-service.exe') -ArgumentList '--status' -Credential $credential -LoadUserProfile -WorkingDirectory $env:SystemRoot -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $userOut -RedirectStandardError $userErr
    if ($process.ExitCode -ne 0) { if(Test-Path $userErr){Get-Content $userErr}; throw 'Non-administrator client could not authenticate the service' }
    $health = Get-Content -LiteralPath $userOut -Raw | ConvertFrom-Json
    if ($health.protocol -ne 2 -or -not $health.systemTunnel) { throw 'Unexpected non-administrator health response' }
    Write-Output 'Non-administrator service authentication passed.'
    & (Join-Path $PSScriptRoot 'test-service-tunnel.ps1') -Credential $credential
} finally {
    if ($createdUser) { Remove-LocalUser -Name $testUser }
    if ($installed) { & (Join-Path $PSScriptRoot 'Service.ps1') -Action Remove }
}
