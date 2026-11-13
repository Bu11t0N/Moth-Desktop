#requires -Version 7.0
param([string]$OutputDirectory=(Join-Path (Split-Path $PSScriptRoot) 'target/upgrade-baselines'))
$ErrorActionPreference='Stop'
$pins=@{
    '0.3.1'='3c9155623b46ab42533eb8d2fe070986bb1d0cdbc185f162c12d207639460816'
    '0.3.2'='84b67ac6d5776cf81c097317874f67d497d09e2e263b99673482f9caaf12ad69'
}
$releases=Invoke-RestMethod -Uri 'https://api.github.com/repos/Bu11t0N/Moth-Desktop/releases?per_page=30'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
foreach($version in $pins.Keys){
    $name="Moth-$version-windows-x64"
    $destination=Join-Path $OutputDirectory $name
    if(Test-Path -LiteralPath $destination){throw 'Baseline output already exists.'}
    $assets=@($releases | Where-Object {$_.tag_name -in @($version,"v$version")} | ForEach-Object {$_.assets} | Where-Object name -eq "$name.zip")
    if($assets.Count -ne 1 -or $assets[0].browser_download_url -notlike 'https://github.com/Bu11t0N/Moth-Desktop/releases/download/*'){throw 'Expected baseline release asset missing.'}
    $archive=Join-Path $OutputDirectory "$name.zip"
    Invoke-WebRequest -Uri $assets[0].browser_download_url -OutFile $archive
    if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins[$version]){throw 'Baseline ZIP checksum mismatch.'}
    Expand-Archive -LiteralPath $archive -DestinationPath $destination
}
