param([switch]$SkipApp)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
Set-Location $Root

$Version = (Select-String -Path "$Root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
Write-Host ""
Write-Host "=== PrismDisk $Version build ===" -ForegroundColor Cyan

if (-not $SkipApp) {
    Write-Host "[1/3] Building PrismDisk (release)..." -ForegroundColor Yellow
    cargo build --release -p prismdisk
    if ($LASTEXITCODE -ne 0) { throw "App build failed (exit $LASTEXITCODE)" }
}

$App = Join-Path $Root "target\release\PrismDisk.exe"
if (-not (Test-Path $App)) { throw "PrismDisk.exe not found at '$App'. Run without -SkipApp." }

Write-Host "[2/3] Building setup wizard (release, app embedded)..." -ForegroundColor Yellow
$env:PRISMDISK_PAYLOAD = $App
(Get-Item "$Root\setup\build.rs").LastWriteTime = Get-Date
cargo build --release -p prismdisk-setup
if ($LASTEXITCODE -ne 0) { throw "Setup build failed (exit $LASTEXITCODE)" }

Write-Host "[3/3] Packaging..." -ForegroundColor Yellow
$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Out = Join-Path $Dist "PrismDisk-Setup-$Version.exe"
Copy-Item (Join-Path $Root "target\release\PrismDisk-Setup.exe") $Out -Force

$SizeMB = [math]::Round((Get-Item $Out).Length / 1MB, 2)
Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Green
Write-Host "Installer: $Out ($SizeMB MB)"
Write-Host "Silent install:   `"$Out`" --silent [--dir=C:\Path] [--no-desktop] [--no-context-menu] [--no-admin]"
