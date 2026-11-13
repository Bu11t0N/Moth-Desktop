#requires -Version 7.0
param([string]$OutputDirectory=(Split-Path (Split-Path $PSScriptRoot)),[switch]$Desktop,[string]$BuildDirectory=(Join-Path (Split-Path $PSScriptRoot) 'target/release'))
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$version=(Get-Content -LiteralPath (Join-Path $repo 'package.json') -Raw | ConvertFrom-Json).version
if($version -notmatch '^\d+\.\d+\.\d+$'){throw 'Invalid release version'}
$output=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
$name="Moth-$version-windows-x64"
$package=Join-Path $output $name
$zip=Join-Path $output "$name.zip"
if((Test-Path -LiteralPath $package) -or (Test-Path -LiteralPath $zip)){throw 'Release output already exists; preserve published versions.'}
New-Item -ItemType Directory -Path $package | Out-Null
foreach($file in @('moth-desktop.exe','moth-service.exe')){
    Copy-Item -LiteralPath (Join-Path $BuildDirectory $file) -Destination $package
}
foreach($directory in @('engines','notices')){
    Copy-Item -LiteralPath (Join-Path $BuildDirectory $directory) -Destination $package -Recurse
}
foreach($file in @('Service.ps1','Install-Service.ps1','Install-Service.cmd')){
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $package
}
Copy-Item -LiteralPath (Join-Path $repo 'docs\RELEASE.ru.txt') -Destination (Join-Path $package 'README.txt')
$serviceVersion=& (Join-Path $package 'moth-service.exe') --version
if($LASTEXITCODE -ne 0 -or $serviceVersion -ne $version){throw 'Packaged service version mismatch'}
$guiVersion=[Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $package 'moth-desktop.exe')).ProductVersion
if($guiVersion -ne $version){throw 'Packaged desktop version mismatch'}
$pins=Get-Content -LiteralPath (Join-Path $repo 'engines.lock.json') -Raw | ConvertFrom-Json
foreach($core in @('xray','sing-box')){
    $expected=$pins.engines.$core.platforms.'win32-x64'.binarySha256
    if((Get-FileHash -LiteralPath (Join-Path $package "engines/$core.exe") -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected){throw 'Packaged core checksum mismatch'}
}

$hashes=@(Get-ChildItem -LiteralPath $package -File -Recurse | Sort-Object FullName | ForEach-Object {
    $relative=$_.FullName.Substring($package.Length+1).Replace('\','/')
    "$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $relative"
})
Set-Content -LiteralPath (Join-Path $package 'SHA256SUMS') -Value $hashes -Encoding utf8NoBOM
Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zip -CompressionLevel Optimal
if($Desktop){
    $desktopRoot=Join-Path ([Environment]::GetFolderPath('Desktop')) 'Moth'
    if((Test-Path -LiteralPath $desktopRoot) -and ((Get-Item -LiteralPath $desktopRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Desktop Moth must not be a reparse point'}
    New-Item -ItemType Directory -Path $desktopRoot -Force | Out-Null
    $destination=Join-Path $desktopRoot $version
    if(Test-Path -LiteralPath $destination){throw 'Desktop version directory already exists; no user files were overwritten'}
    Expand-Archive -LiteralPath $zip -DestinationPath $destination
    foreach($line in $hashes){
        $hash,$relative=$line -split '  ',2
        if((Get-FileHash -LiteralPath (Join-Path $destination $relative) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash){throw 'Extracted release hash mismatch'}
    }
    $shell=New-Object -ComObject WScript.Shell
    $shortcut=$shell.CreateShortcut((Join-Path $desktopRoot 'Moth.lnk'))
    $shortcut.TargetPath=Join-Path $destination 'moth-desktop.exe'
    $shortcut.WorkingDirectory=$destination
    $shortcut.Description="Moth Desktop $version"
    $shortcut.Save()
    Write-Output "Extracted and verified: $destination"
}
Write-Output "Release ZIP: $zip"
