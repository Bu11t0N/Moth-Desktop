#requires -Version 7.0
param([Parameter(Mandatory)][string]$FixtureDirectory)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Assert-DisposableVm.ps1')
Assert-DisposableWindowsVm
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Hosted disposable CI is required.' }
$fixture=(Resolve-Path -LiteralPath $FixtureDirectory).Path
$root=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Moth Desktop'
if ((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath $root)) { throw 'A fresh Windows VM is required.' }
$user='MothCiUpdate'
if (Get-LocalUser -Name $user -ErrorAction SilentlyContinue) { throw 'Acceptance account already exists.' }
$created=$false
$installed=$false
$guiProcess=$null
$helperProcess=$null
$serverProcess=$null
$jobs=@()
function Wait-NoGui {
    $deadline=[DateTime]::UtcNow.AddSeconds(20)
    while (Get-Process -Name moth-desktop,moth-monitor -ErrorAction SilentlyContinue) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Acceptance GUI did not exit.' }
        Start-Sleep -Milliseconds 100
    }
}
function Install-Setup([string]$Path) {
    $process=Start-Process -FilePath $Path -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Actual baseline NSIS installation failed.' }
}
try {
    Install-Setup (Join-Path $fixture 'baseline/Moth-0.3.4-windows-x64-setup.exe')
    $installed=$true
    $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
    $listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
    $identifier=[Guid]::NewGuid().ToString('D')
    $link='vless://{0}@127.0.0.1:{1}?encryption=none&security=none&type=tcp#Updater-retention' -f $identifier,$port
    $interface=(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1).InterfaceAlias
    $config=@{log=@{loglevel='none'};inbounds=@(@{listen='127.0.0.1';port=$port;protocol='vless';settings=@{decryption='none';clients=@(@{id=$identifier})}});outbounds=@(@{protocol='freedom';settings=@{domainStrategy='UseIPv4';finalRules=@(@{action='allow'})};streamSettings=@{sockopt=@{interface=$interface}}})}
    $serverConfig=Join-Path $fixture 'server.json'
    $config | ConvertTo-Json -Depth 12 -Compress | Set-Content -LiteralPath $serverConfig -Encoding utf8NoBOM
    $serverBinary=Join-Path $fixture 'xray-fixture.exe'
    Copy-Item -LiteralPath (Join-Path $root 'engines/xray.exe') -Destination $serverBinary
    $serverProcess=Start-Process -FilePath $serverBinary -ArgumentList "run -config `"$serverConfig`"" -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $fixture 'server.out') -RedirectStandardError (Join-Path $fixture 'server.err')
    $password=ConvertTo-SecureString ([Guid]::NewGuid().ToString('N')+'aA1!') -AsPlainText -Force
    New-LocalUser -Name $user -Password $password -AccountNeverExpires -PasswordNeverExpires | Out-Null
    $created=$true
    $sid=(Get-LocalUser -Name $user).SID
    if (-not (Get-LocalGroupMember -SID 'S-1-5-32-545' | Where-Object {$_.SID.Value -eq $sid.Value})) {
        Add-LocalGroupMember -SID 'S-1-5-32-545' -Member "$env:COMPUTERNAME\$user"
    }
    # Initialize the new account's actual Windows profile before starting a
    # WebView. Custom ProcessStartInfo environments otherwise inherit the CI
    # administrator's USERPROFILE/LOCALAPPDATA/TEMP despite LoadUserProfile.
    $credential=[PSCredential]::new("$env:COMPUTERNAME\$user",$password)
    $initialize=Start-Process -FilePath (Join-Path $env:SystemRoot 'System32/cmd.exe') -ArgumentList '/c exit 0' -Credential $credential -LoadUserProfile -WorkingDirectory $env:SystemRoot -WindowStyle Hidden -Wait -PassThru
    if ($initialize.ExitCode -ne 0) { throw 'Acceptance user profile initialization failed.' }
    $initialize.Dispose()
    $profileImage=(Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$($sid.Value)").ProfileImagePath
    $profileImage=[Environment]::ExpandEnvironmentVariables($profileImage)
    if (-not [IO.Path]::IsPathFullyQualified($profileImage) -or -not (Test-Path -LiteralPath $profileImage -PathType Container)) { throw 'Actual Windows acceptance profile is unavailable.' }
    $userEnvironment=@{
        USERPROFILE=$profileImage
        LOCALAPPDATA=(Join-Path $profileImage 'AppData/Local')
        APPDATA=(Join-Path $profileImage 'AppData/Roaming')
        TEMP=(Join-Path $profileImage 'AppData/Local/Temp')
        TMP=(Join-Path $profileImage 'AppData/Local/Temp')
        USERNAME=$user
        USERDOMAIN=$env:COMPUTERNAME
    }
    $data=Join-Path $fixture 'desktop-data'
    New-Item -ItemType Directory -Path $data | Out-Null
    $acl=Get-Acl -LiteralPath $data
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'Modify','ContainerInherit, ObjectInherit','None','Allow'))
    Set-Acl -LiteralPath $data -AclObject $acl
    $originalProfileHash=$null
    foreach ($case in @('metadata-rejection','download-rejection','rollback','success')) {
        Wait-NoGui
        Set-Content -LiteralPath (Join-Path $fixture 'mode') -Value $case -NoNewline
        $evidence=Join-Path $data $case
        New-Item -ItemType Directory -Path $evidence | Out-Null
        $info=[Diagnostics.ProcessStartInfo]::new((Join-Path $root 'moth-desktop.exe'))
        $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.LoadUserProfile=$true
        $info.UserName=$user;$info.Domain=$env:COMPUTERNAME;$info.Password=$password
        $info.WorkingDirectory=$env:SystemRoot;$info.RedirectStandardInput=$true
        $info.Environment['MOTH_NATIVE_UPDATE_TEST']='1'
        $info.Environment['MOTH_NATIVE_UPDATE_EVIDENCE']=$evidence
        $info.Environment['MOTH_NATIVE_UPDATE_EXPECT']=$case
        $info.Environment['MOTH_DEV_DATA_DIR']=$data
        foreach ($entry in $userEnvironment.GetEnumerator()) { $info.Environment[$entry.Key]=$entry.Value }
        $guiProcess=[Diagnostics.Process]::Start($info)
        # Synthetic credential goes only over a private stdin pipe, not argv/logs.
        $guiProcess.StandardInput.Write($link)
        $guiProcess.StandardInput.Close()
        $deadline=[DateTime]::UtcNow.AddMinutes(12)
        $authorized=$false
        while (-not (Test-Path -LiteralPath (Join-Path $evidence 'result.json'))) {
            if (-not $authorized -and $guiProcess.HasExited) { throw "Native GUI exited before update authorization: $($guiProcess.ExitCode)." }
            if (Test-Path -LiteralPath (Join-Path $evidence 'error.txt')) { throw (Get-Content -LiteralPath (Join-Path $evidence 'error.txt') -Raw) }
            $request=Join-Path $evidence 'privileged-request'
            if (-not $authorized -and (Test-Path -LiteralPath $request)) {
                $identifier=(Get-Content -LiteralPath $request -Raw).Trim()
                $parsed=[Guid]::Empty
                if (-not [Guid]::TryParseExact($identifier,'D',[ref]$parsed) -or $identifier -cne $parsed.ToString('D')) { throw 'Invalid opaque acceptance request.' }
                $requestDirectory=Join-Path $data "updates/$identifier"
                $stopped=(& (Join-Path $root 'moth-service.exe') --tunnel-status) | ConvertFrom-Json
                if ($LASTEXITCODE -ne 0 -or $stopped.phase -ne 'disconnected') { throw 'GUI did not confirm VPN stop before privileged update.' }
                $helper=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Moth Desktop/maintenance/0.3.4/moth-maintenance.exe'
                $helperProcess=Start-Process -FilePath $helper -ArgumentList "--apply `"$requestDirectory`"" -WindowStyle Hidden -PassThru
                $jobs+=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) "Moth Desktop/updates/$identifier"
                $authorized=$true
            }
            if ([DateTime]::UtcNow -gt $deadline) { throw 'Native signed update timed out.' }
            Start-Sleep -Milliseconds 100
        }
        Wait-NoGui
        $result=Get-Content -LiteralPath (Join-Path $evidence 'result.json') -Raw | ConvertFrom-Json
        if (-not $result.ordinaryUser -or -not $result.profilesUnchanged) { throw 'Ordinary-user profile retention failed.' }
        if ($case -in @('metadata-rejection','download-rejection')) {
            if ($authorized -or -not $result.rejected) { throw 'Changed/unsigned content reached installation.' }
        } else {
            if (-not $authorized -or -not $result.restarted -or -not $result.componentsHealthy) { throw 'Actual coordinated update/restart failed.' }
            if (-not $helperProcess.WaitForExit(15000) -or (($helperProcess.ExitCode -eq 0) -ne ($case -eq 'success'))) { throw 'Privileged updater outcome did not match native evidence.' }
            $monitor=Get-Content -LiteralPath (Join-Path $evidence 'monitor-result.json') -Raw | ConvertFrom-Json
            if (-not $monitor.ordinaryUser -or $monitor.phase -ne $(if($case -eq 'success'){'complete'}else{'failed'})) { throw 'Native ordinary-user monitor failed.' }
            $job=$jobs[-1]
            if (-not (Test-Path -LiteralPath (Join-Path $job 'installing.event'))) { throw 'Actual installer was not reached.' }
            if ($case -eq 'rollback' -and (-not (Test-Path -LiteralPath (Join-Path $job 'rollingBack.event')) -or -not (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Moth Desktop/acceptance-poststart-fixture')))) { throw 'Rollback did not reach the forced post-start failure.' }
            $helperProcess.Dispose();$helperProcess=$null
        }
        $hash=(Get-FileHash -LiteralPath (Join-Path $data 'profiles.dpapi') -Algorithm SHA256).Hash
        if ($originalProfileHash -and $hash -cne $originalProfileHash) { throw 'Updater modified account-bound DPAPI bytes.' }
        $originalProfileHash=$hash
        $expected=if($case -eq 'success'){'0.3.5'}else{'0.3.4'}
        $health=(& (Join-Path $root 'moth-service.exe') --status) | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0 -or $health.version -ne $expected -or @($health.engines | Where-Object {-not $_.available}).Count) { throw 'Installed GUI/service/core version verification failed.' }
        $guiProcess.Dispose();$guiProcess=$null
        Write-Output "Actual ordinary-user signed update acceptance passed: $case"
    }
} catch {
    foreach ($payload in Get-ChildItem -LiteralPath $env:ProgramFiles -Directory -Filter '*.moth-payload') {
        $reason=Join-Path $payload.FullName 'maintenance-error.txt'
        if (Test-Path -LiteralPath $reason -PathType Leaf) { Get-Content -LiteralPath $reason -TotalCount 10 }
    }
    foreach ($job in $jobs) {
        if (Test-Path -LiteralPath (Join-Path $job 'status.json')) { Get-Content -LiteralPath (Join-Path $job 'status.json') }
        Get-ChildItem -LiteralPath $job -Filter '*.event' | Select-Object -ExpandProperty Name
        foreach ($log in Get-ChildItem -LiteralPath $job -Filter '*.command.log') { Get-Content -LiteralPath $log.FullName -Tail 40 }
    }
    throw
} finally {
    if ($guiProcess) { if (-not $guiProcess.HasExited) {$guiProcess.Kill();$guiProcess.WaitForExit()};$guiProcess.Dispose() }
    if ($helperProcess) { if (-not $helperProcess.HasExited) {$helperProcess.Kill();$helperProcess.WaitForExit()};$helperProcess.Dispose() }
    if ($serverProcess) { if (-not $serverProcess.HasExited) {$serverProcess.Kill();$serverProcess.WaitForExit()};$serverProcess.Dispose() }
    if ($installed -and (Test-Path -LiteralPath (Join-Path $root 'Uninstall.exe'))) {
        Wait-NoGui
        $remove=Start-Process -FilePath (Join-Path $root 'Uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
        if ($remove.ExitCode -ne 0) { throw 'Signed-update acceptance uninstall failed.' }
    }
    if ($created) { Remove-LocalUser -Name $user }
}
