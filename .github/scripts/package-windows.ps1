param(
    [Parameter(Mandatory = $true)]
    [string]$Target,
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$TargetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root "target" }
$BinaryRoot = Join-Path $TargetRoot "$Target/release"
$Package = "angry-birds-stella-rehost-$Version-$Target"
$DistRoot = Join-Path $Root "dist"
$PackageRoot = Join-Path $DistRoot $Package
$Archive = Join-Path $DistRoot "$Package.zip"

if (Test-Path $PackageRoot) {
    Remove-Item -Recurse -Force $PackageRoot
}
New-Item -ItemType Directory -Force $PackageRoot | Out-Null

foreach ($Binary in @("stella-app", "stella-headless", "stella-mp3-audit", "stella-tool")) {
    Copy-Item (Join-Path $BinaryRoot "$Binary.exe") (Join-Path $PackageRoot "$Binary.exe")
}

Copy-Item (Join-Path $Root "README.md") (Join-Path $PackageRoot "README.md")
Copy-Item (Join-Path $Root "LICENSE") (Join-Path $PackageRoot "LICENSE")

$Commit = if ($env:GITHUB_SHA) { $env:GITHUB_SHA } else { git -C $Root rev-parse HEAD }
@(
    "Version: $Version"
    "Target: $Target"
    "Commit: $Commit"
    "Rust: $(rustc --version)"
) | Set-Content -Encoding utf8NoBOM (Join-Path $PackageRoot "BUILD-INFO.txt")

if (Test-Path $Archive) {
    Remove-Item -Force $Archive
}
Compress-Archive -Path (Join-Path $PackageRoot "*") -DestinationPath $Archive -CompressionLevel Optimal
Remove-Item -Recurse -Force $PackageRoot
Write-Output $Archive
