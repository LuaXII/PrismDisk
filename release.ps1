param([switch]$Draft)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
Set-Location $Root

$Version = (Select-String -Path "$Root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$Tag = "v$Version"
Write-Host "=== Releasing PrismDisk $Version ===" -ForegroundColor Cyan

$Changelog = Get-Content "$Root\CHANGELOG.md" -Raw
$Pattern = "(?ms)^## " + [regex]::Escape($Version) + "[^\n]*\n(.*?)(?=^## |\z)"
$Match = [regex]::Match($Changelog, $Pattern)
if (-not $Match.Success) { throw "CHANGELOG.md has no '## $Version' section. Add release notes first." }
$Notes = $Match.Groups[1].Value.Trim()

if (git tag --list $Tag) { throw "Tag $Tag already exists. Bump the version in Cargo.toml first." }
if (git status --porcelain) { throw "Commit your changes before releasing." }

Copy-Item "$Root\CHANGELOG.md" "$Root\docs\CHANGELOG.md" -Force
if (git status --porcelain docs) {
    git add docs/CHANGELOG.md
    git commit -m "Update website changelog for $Version"
}

& "$Root\build.ps1"
if ($LASTEXITCODE -ne 0) { throw "Build failed" }
$Installer = Join-Path $Root "dist\PrismDisk-Setup-$Version.exe"

git tag -a $Tag -m "PrismDisk $Version"
git push origin HEAD
git push origin $Tag

$NotesFile = New-TemporaryFile
Set-Content -Path $NotesFile -Value $Notes -Encoding utf8
$Args = @("release", "create", $Tag, $Installer, "--title", "PrismDisk $Version", "--notes-file", $NotesFile)
if ($Draft) { $Args += "--draft" }
gh @Args
Remove-Item $NotesFile

Write-Host "=== Released $Tag ===" -ForegroundColor Green
