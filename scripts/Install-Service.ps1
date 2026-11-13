#requires -Version 5.1
$ErrorActionPreference='Stop'
try {
    $principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        $process=Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"')) -Wait -PassThru
        exit $process.ExitCode
    }
    & (Join-Path $PSScriptRoot 'Service.ps1') -Action Install -PackageDirectory $PSScriptRoot
    Write-Host 'Moth service installed. Open moth-desktop.exe and connect.'
    Read-Host 'Press Enter to close'
} catch {
    Write-Host $_.Exception.Message -ForegroundColor Red
    Read-Host 'Press Enter to close'
    exit 1
}
