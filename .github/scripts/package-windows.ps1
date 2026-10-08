param(
    [Parameter(Mandatory = $true)]
    [string]$Target,
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
python (Join-Path $PSScriptRoot "package-desktop.py") $Target $Version
if ($LASTEXITCODE -ne 0) {
    throw "Desktop packaging failed with exit code $LASTEXITCODE"
}
