# Real service + standard-user data-plane acceptance. Disposable CI VM only.
#requires -Version 7.0
param([Parameter(Mandatory)][PSCredential]$Credential)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows'){throw 'Disposable Windows VM required'}
$serviceExe=Join-Path $env:ProgramFiles 'Moth Desktop\moth-service.exe'
function Invoke-Client([string]$Argument,[string]$InputText='', [PSCredential]$AsUser=$null){
    $info=[Diagnostics.ProcessStartInfo]::new($serviceExe,$Argument)
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=$env:SystemRoot
    $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    if($AsUser){$info.UserName=$AsUser.UserName.Split('\')[-1];$info.Domain=$env:COMPUTERNAME;$info.Password=$AsUser.Password;$info.LoadUserProfile=$true}
    $process=[Diagnostics.Process]::Start($info)
    try {
        $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($InputText);$process.StandardInput.Close()
        if(-not $process.WaitForExit(35000)){$process.Kill();throw 'Service client timed out'}
        if($process.ExitCode -ne 0){throw "Service client failed: $($stderr.GetAwaiter().GetResult())"}
        return ($stdout.GetAwaiter().GetResult() | ConvertFrom-Json)
    } finally {$process.Dispose()}
}
$route=Find-NetRoute -RemoteIPAddress '1.1.1.1' | Where-Object DestinationPrefix | Select-Object -First 1
$interface=(Get-NetAdapter -InterfaceIndex $route.InterfaceIndex).Name
$cloudflare=(Resolve-DnsName 'cp.cloudflare.com' -Type A -DnsOnly | Where-Object IPAddress | Select-Object -First 1).IPAddress
if(-not $cloudflare -or -not $interface){throw 'Fixture upstream discovery failed'}
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
$id=[Guid]::NewGuid().ToString()
$config=@{
    log=@{loglevel='none'}
    dns=@{hosts=@{'cp.cloudflare.com'=$cloudflare}}
    inbounds=@(@{listen='127.0.0.1';port=$port;protocol='vless';settings=@{decryption='none';clients=@(@{id=$id})}})
    outbounds=@(@{protocol='freedom';settings=@{domainStrategy='UseIPv4';finalRules=@(@{action='allow'})};streamSettings=@{sockopt=@{interface=$interface}}})
} | ConvertTo-Json -Depth 12 -Compress
$info=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSScriptRoot '..\.local\engines\win32-x64\xray.exe'),'run -format json -config stdin:')
$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
$fixture=[Diagnostics.Process]::Start($info)
try {
    $fixture.StandardInput.Write($config);$fixture.StandardInput.Close()
    Start-Sleep -Milliseconds 500
    if($fixture.HasExited){throw 'Fixture server failed to start'}
    $link="vless://$id@127.0.0.1:$port`?security=none&type=tcp"
    $connected=Invoke-Client '--connect' $link $Credential
    if($connected.phase -ne 'connected' -or -not $connected.ownedByCaller){throw 'Standard-user VPN did not pass real TUN HTTPS/DNS verification'}
    $dns=Resolve-DnsName 'example.com' -Type A -Server '172.19.0.2' -DnsOnly -NoHostsFile
    if(-not @($dns | Where-Object IPAddress).Count){throw 'Explicit DNS query through the TUN gateway failed'}
    $other=Invoke-Client '--tunnel-status'
    if($other.phase -ne 'connected' -or $other.ownedByCaller){throw 'Connection SID ownership was lost'}
    $denied=Invoke-Client '--disconnect'
    if($denied.kind -ne 'failure' -or $denied.code -ne 'notOwner'){throw 'Another account disconnected the tunnel'}
    $off=Invoke-Client '--disconnect' '' $Credential
    if($off.phase -ne 'disconnected'){throw 'Disconnect failed'}
    $again=Invoke-Client '--connect' $link $Credential
    if($again.phase -ne 'connected'){throw 'Service reconnect failed'}
    $servicePid=(Get-CimInstance Win32_Service -Filter "Name='MothDesktopService'").ProcessId
    $child=Get-CimInstance Win32_Process -Filter "ParentProcessId=$servicePid" | Where-Object Name -eq 'xray.exe'
    if(@($child).Count -ne 1){throw 'Expected one service-owned upstream process'}
    Stop-Process -Id $child.ProcessId -Force
    Start-Sleep -Seconds 3
    $failed=Invoke-Client '--tunnel-status' '' $Credential
    if($failed.phase -ne 'failed'){throw 'Engine failure was not reported'}
    $recovered=Invoke-Client '--connect' $link $Credential
    if($recovered.phase -ne 'connected'){throw 'Reconnect after engine failure failed'}
    Stop-Service MothDesktopService
    (Get-Service MothDesktopService).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(15))
    Start-Sleep -Seconds 2
    if(@(Get-NetAdapter | Where-Object {$_.Name -eq 'Moth' -and $_.Status -eq 'Up'}).Count){throw 'TUN remained active after service shutdown'}
    Start-Service MothDesktopService
    (Get-Service MothDesktopService).WaitForStatus('Running',[TimeSpan]::FromSeconds(20))
    $fresh=Invoke-Client '--tunnel-status' '' $Credential
    if($fresh.phase -ne 'disconnected'){throw 'Restart did not reset connection state'}
    # Native debug GUI, real window events/single-instance process and the same
    # production service. No UI test driver is compiled into release packages.
    $nativeData=Join-Path (Split-Path $PSScriptRoot) ('.local\native-tray-'+[Guid]::NewGuid().ToString('N'))
    $nativeInfo=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSScriptRoot '..\target\debug\moth-desktop.exe'))
    $nativeInfo.UseShellExecute=$false;$nativeInfo.CreateNoWindow=$true
    $nativeInfo.RedirectStandardInput=$true
    $nativeInfo.Environment['MOTH_NATIVE_TRAY_TEST']='1'
    $nativeInfo.Environment['MOTH_DEV_DATA_DIR']=$nativeData
    $native=[Diagnostics.Process]::Start($nativeInfo)
    try {
        $native.StandardInput.Write($link);$native.StandardInput.Close()
        if(-not $native.WaitForExit(90000)){$native.Kill();throw 'Native tray lifecycle timed out'}
        $evidencePath=Join-Path $nativeData 'native-tray-result.json'
        if(-not (Test-Path -LiteralPath $evidencePath)){throw 'Native tray lifecycle evidence missing'}
        $evidence=Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
        if($native.ExitCode -ne 0 -or -not $evidence.trayRegistered -or -not $evidence.closeKeptVpn -or -not $evidence.secondLaunchRestored -or -not $evidence.exitDisconnected){
            if($evidence.error){Write-Output $evidence.error}
            throw 'Native tray lifecycle assertions failed'
        }
        $afterExit=Invoke-Client '--tunnel-status'
        if($afterExit.phase -ne 'disconnected'){throw 'Tray Exit left the VPN running'}
        if(@(Get-NetAdapter | Where-Object {$_.Name -eq 'Moth' -and $_.Status -eq 'Up'}).Count){throw 'Tray Exit left TUN active'}
        Write-Output 'Native tray registration, close-to-tray with live VPN, second-instance reopen and Exit disconnect passed.'
    } finally {$native.Dispose()}
    Write-Output 'Standard-user connect, real HTTPS + DNS over TUN, SID isolation, reconnect, engine crash and service shutdown passed.'
} finally {
    if(-not $fixture.HasExited){$fixture.Kill();$fixture.WaitForExit()}
    $fixture.Dispose()
}
