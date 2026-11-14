#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$PackageDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$MakeNsis = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$version=(Get-Content -LiteralPath (Join-Path $repo 'package.json') -Raw | ConvertFrom-Json).version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid version.' }
$package=(Resolve-Path -LiteralPath $PackageDirectory).Path
$output=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
$installer=Join-Path $output "Moth-$version-windows-x64-setup.exe"
if (Test-Path -LiteralPath $installer) { throw 'Installer output exists; choose a new output directory.' }
if (-not (Test-Path -LiteralPath $MakeNsis -PathType Leaf)) { throw 'NSIS 3.11 is required.' }
$nsisVersion=& $MakeNsis /VERSION
if ($LASTEXITCODE -ne 0 -or $nsisVersion.Trim() -ne 'v3.11') { throw 'NSIS 3.11 is required.' }
# Build a separate minimal payload; never package a Cargo target tree.
$payload=Join-Path $output ('nsis-payload-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $payload | Out-Null
foreach ($name in @('moth-desktop.exe','moth-service.exe','README.txt')) { Copy-Item -LiteralPath (Join-Path $package $name) -Destination $payload }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Service.ps1') -Destination $payload
foreach ($name in @('engines','notices')) { Copy-Item -LiteralPath (Join-Path $package $name) -Destination $payload -Recurse }
$serviceVersion=& (Join-Path $payload 'moth-service.exe') --version
$guiVersion=[Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $payload 'moth-desktop.exe')).ProductVersion
if ($LASTEXITCODE -ne 0 -or $serviceVersion -ne $version -or $guiVersion -ne $version) { throw 'Installer component version mismatch.' }
$entries=@(Get-ChildItem -LiteralPath $payload -File -Recurse | Sort-Object FullName | ForEach-Object {
    @{path=$_.FullName.Substring($payload.Length+1).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
@{version=$version;files=$entries} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $payload 'desktop-package.json') -Encoding utf8NoBOM
& $MakeNsis /NOCONFIG /V2 "/DVERSION=$version" "/DPAYLOAD=$payload" "/DOUTPUT=$installer" (Join-Path $PSScriptRoot 'installer/windows.nsi')
if ($LASTEXITCODE -ne 0) { throw 'NSIS build failed.' }
"$((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($installer))" | Set-Content -LiteralPath ($installer+'.sha256') -Encoding utf8NoBOM
Write-Output "Installer: $installer"
