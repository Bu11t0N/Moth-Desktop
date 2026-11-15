#requires -Version 7.0
param([Parameter(Mandatory)][string]$Link,[Parameter(Mandatory)][string]$Target)
$ErrorActionPreference='Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'Disposable Windows CI is required.' }
if (Test-Path -LiteralPath $Link) { throw 'Fixture target already exists.' }
New-Item -ItemType Junction -Path $Link -Target (Resolve-Path -LiteralPath $Target).Path | Out-Null
