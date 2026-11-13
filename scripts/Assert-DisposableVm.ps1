# Shared guard for destructive service/network acceptance. Never call on the host.
function Assert-DisposableWindowsVm([switch]$DisposableVm) {
    if ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_OS -eq 'Windows') { return }
    if ($DisposableVm) {
        $computer = Get-CimInstance Win32_ComputerSystem
        if ($computer.Manufacturer -match 'VMware' -and $computer.Model -match 'VMware|Virtual') {
            # Consumed only by debug/test drivers after this hardware check.
            $env:MOTH_DISPOSABLE_VM = '1'
            return
        }
    }
    throw 'Explicit disposable VMware VM or Windows CI runner required.'
}
