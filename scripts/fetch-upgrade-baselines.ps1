#requires -Version 7.0
param([string]$OutputDirectory=(Join-Path (Split-Path $PSScriptRoot) 'target/upgrade-baselines'))
$ErrorActionPreference='Stop'
$pins=@{
    '0.3.5'='8b0113be2afa42e1b71a551abc0c984aba6b1fa8d57ab19f07aa5d65044f2dfd'
    '0.3.4'='34924388364e8161e169e2966deb5b573962fed39d9a65e7d0599449c37c0f23'
    '0.3.3'='cb455b49a9c567a3292f38f2297c6fd85b85f0f64c66d528a148372aee0418f4'
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
