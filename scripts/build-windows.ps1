#requires -Version 7.0
param([string]$TargetDirectory=(Join-Path (Split-Path $PSScriptRoot) 'target'))
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$target=[IO.Path]::GetFullPath($TargetDirectory)
if($env:RUSTFLAGS){throw 'Use a clean RUSTFLAGS environment for the release build; CARGO_ENCODED_RUSTFLAGS is preserved.'}
$oldFlags=$env:CARGO_ENCODED_RUSTFLAGS
$oldTarget=$env:CARGO_TARGET_DIR
$flags=@()
if($oldFlags){$flags+=($oldFlags -split [char]31)}
# Compiler diagnostics/panic locations must not disclose the workstation or account.
$locations=@(
    @([Environment]::GetFolderPath('UserProfile'),'/build/home'),
    @($repo,'/moth'),
    @($env:CARGO_HOME,'/cargo'),
    @($env:RUSTUP_HOME,'/rustup'),
    @($target,'/target')
)
foreach($location in $locations){if($location[0]){$flags+="--remap-path-prefix=$($location[0])=$($location[1])"}}
try {
    $env:CARGO_ENCODED_RUSTFLAGS=$flags -join [char]31
    $env:CARGO_TARGET_DIR=$target
    Push-Location $repo
    try {
        cargo build -p moth-service --release --locked
        if($LASTEXITCODE -ne 0){throw 'Service release build failed.'}
        npm.cmd run desktop:build -- --no-bundle
        if($LASTEXITCODE -ne 0){throw 'Desktop release build failed.'}
    } finally {Pop-Location}
    foreach($name in @('moth-desktop.exe','moth-service.exe')){
        $bytes=[IO.File]::ReadAllBytes((Join-Path $target "release/$name"))
        foreach($encoding in @([Text.Encoding]::UTF8,[Text.Encoding]::Unicode)){
            $text=$encoding.GetString($bytes)
            foreach($location in $locations){
                if($location[0] -and $text.IndexOf($location[0],[StringComparison]::OrdinalIgnoreCase) -ge 0){throw 'Release binary contains an unremapped build path.'}
            }
        }
    }
    Write-Output 'Windows release binaries built and checked for local build paths.'
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS=$oldFlags
    $env:CARGO_TARGET_DIR=$oldTarget
}
