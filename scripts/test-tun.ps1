#requires -Version 7.0
$ErrorActionPreference='Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'System tunnel tests require a disposable GitHub-hosted Windows VM.' }
$interfaces=@(Get-NetAdapter | Where-Object Name -ne 'Moth' | Select-Object -ExpandProperty ifIndex)
function Physical-Dns { @(Get-DnsClientServerAddress | Where-Object InterfaceIndex -in $interfaces | ForEach-Object { "$($_.InterfaceIndex)|$($_.AddressFamily)|$($_.ServerAddresses -join ',')" } | Sort-Object) -join "`n" }
function Physical-Routes { @(Get-NetRoute | Where-Object InterfaceIndex -in $interfaces | ForEach-Object { "$($_.InterfaceIndex)|$($_.DestinationPrefix)|$($_.NextHop)|$($_.RouteMetric)" } | Sort-Object) }
$dns=Physical-Dns
$routes=Physical-Routes
try {
    $env:MOTH_TUN_ACCEPTANCE='1'
    cargo test -p moth-engine windows_system_tun_roundtrip --locked -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Real TUN data-plane test failed' }
} finally { Remove-Item Env:\MOTH_TUN_ACCEPTANCE -ErrorAction SilentlyContinue }
if (@(Get-NetAdapter | Where-Object {$_.Name -eq 'Moth' -and $_.Status -eq 'Up'}).Count) {throw 'TUN adapter remained active after disconnect'}
if ((Physical-Dns) -ne $dns) {throw 'Physical adapter DNS changed'}
$remaining=Physical-Routes
foreach($route in $routes) {if($route -notin $remaining){throw 'A pre-existing physical route was removed or modified'}}
Write-Output 'TUN TCP roundtrip, disconnect, reconnect, adapter teardown and physical DNS/routes preservation passed.'
