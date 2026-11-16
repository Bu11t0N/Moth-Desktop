#requires -Version 7.0
param([Parameter(Mandatory)][string]$Installer, [Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Assert-DisposableVm.ps1')
Assert-DisposableWindowsVm
if (Get-Service MothDesktopService -ErrorAction SilentlyContinue) { throw 'Clean disposable VM required.' }
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
$out=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $out | Out-Null
function Window {
    $deadline=[DateTime]::UtcNow.AddSeconds(45)
    do {
        $windows=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
        foreach($w in $windows) { if($w.Current.Name -like '*Moth*' -and $w.Current.ClassName -eq '#32770') { return $w } }
        Start-Sleep -Milliseconds 200
    } while([DateTime]::UtcNow -lt $deadline)
    throw 'Installer window unavailable.'
}
function Text($w) {
    $parts=[Collections.Generic.List[string]]::new()
    foreach($el in $w.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)) {
        $parts.Add($el.Current.Name)
        $pattern=$null
        if($el.TryGetCurrentPattern([Windows.Automation.TextPattern]::Pattern,[ref]$pattern)) { $parts.Add($pattern.DocumentRange.GetText(-1)) }
        $pattern=$null
        if($el.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern,[ref]$pattern)) { $parts.Add($pattern.Current.Value) }
    }
    return $parts -join "`n"
}
function Capture($w,[string]$name) {
    $text=Text $w
    [IO.File]::WriteAllText((Join-Path $out "$name.txt"),$text,[Text.UTF8Encoding]::new($false))
    $r=$w.Current.BoundingRectangle
    $bitmap=[Drawing.Bitmap]::new([int]$r.Width,[int]$r.Height)
    $g=[Drawing.Graphics]::FromImage($bitmap)
    try { $g.CopyFromScreen([int]$r.X,[int]$r.Y,0,0,$bitmap.Size); $bitmap.Save((Join-Path $out "$name.png")) } finally { $g.Dispose(); $bitmap.Dispose() }
    return $text
}
function Next($w) {
    $condition=[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'1')
    $button=$w.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
    if(-not $button -or -not $button.Current.IsEnabled) { throw 'Installer action unavailable.' }
    $button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Wait-Text([string]$expected) {
    $deadline=[DateTime]::UtcNow.AddSeconds(120)
    do { $w=Window; if((Text $w).Contains($expected)) { return $w }; Start-Sleep -Milliseconds 250 } while([DateTime]::UtcNow -lt $deadline)
    throw "Installer page not reached: $expected"
}
Start-Process -FilePath ([IO.Path]::GetFullPath($Installer)) -WindowStyle Normal | Out-Null
$w=Wait-Text 'Установка приложения, системной службы и проверенных ядер VPN.'
$null=Capture $w 'welcome'
Next $w
$w=Wait-Text 'GNU GENERAL PUBLIC LICENSE'
$license=Capture $w 'license'
$source=[IO.File]::ReadAllText((Join-Path (Split-Path $PSScriptRoot) 'LICENSE'))
function Normalize([string]$value) { return ($value -replace '\s+',' ').Trim() }
if(-not (Normalize $license).Contains((Normalize $source))) { throw 'Displayed license differs from source.' }
Next $w
$w=Wait-Text 'Приложение установлено. Откройте Moth Desktop'
$null=Capture $w 'finish'
Next $w
$uninstaller=Join-Path $env:ProgramFiles 'Moth Desktop/Uninstall.exe'
Start-Process -FilePath $uninstaller -WindowStyle Normal | Out-Null
$w=Wait-Text 'Будут удалены приложение и системная служба.'
$null=Capture $w 'uninstall-confirm'
Next $w
$deadline=[DateTime]::UtcNow.AddSeconds(120)
do { Start-Sleep -Milliseconds 250; $w=Window; $text=Text $w } while((Get-Service MothDesktopService -ErrorAction SilentlyContinue) -and [DateTime]::UtcNow -lt $deadline)
if(Get-Service MothDesktopService -ErrorAction SilentlyContinue) { throw 'Uninstall did not remove service.' }
$null=Capture $w 'uninstall-finish'
Next $w
Write-Output 'Actual installer text and source license comparison passed; page screenshots captured.'
