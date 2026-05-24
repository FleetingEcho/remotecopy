<#
.SYNOPSIS
  Build and package RemoteCopy GUI for a new Windows release.

.PARAMETER Version
  The release version, e.g. "0.1.2"

.EXAMPLE
  .\scripts\release.ps1 -Version 0.1.2
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$Root        = Split-Path $PSScriptRoot
$GuiDir      = Join-Path $Root "remotecopy-gui"
$TauriDir    = Join-Path $GuiDir "src-tauri"
$OutDir      = Join-Path $Root "release-artifacts"

Write-Host ""
Write-Host "=== RemoteCopy Windows release $Version ===" -ForegroundColor Cyan
Write-Host ""

# ── 1. Bump versions ──────────────────────────────────────────────────────────

Write-Host "[1/3] Bumping versions to $Version..." -ForegroundColor Yellow

# tauri.conf.json
$tauriConf = Join-Path $TauriDir "tauri.conf.json"
(Get-Content $tauriConf -Raw) -replace '"version":\s*"[^"]*"', "`"version`": `"$Version`"" |
    Set-Content $tauriConf -NoNewline

# remotecopy-gui/src-tauri/Cargo.toml
$guiCargo = Join-Path $TauriDir "Cargo.toml"
if (Test-Path $guiCargo) {
    (Get-Content $guiCargo -Raw) -replace '(?m)^version = "[^"]*"', "version = `"$Version`"" |
        Set-Content $guiCargo -NoNewline
}

Write-Host "    Done." -ForegroundColor Green

# ── 2. Build GUI ──────────────────────────────────────────────────────────────

Write-Host "[2/3] Building GUI (bun run tauri build)..." -ForegroundColor Yellow
Push-Location $GuiDir
bun run tauri build --bundles nsis
Pop-Location
Write-Host "    Done." -ForegroundColor Green

# ── 3. Collect and rename artifacts ──────────────────────────────────────────

Write-Host "[3/3] Collecting artifacts..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force $OutDir | Out-Null

# NSIS installer  →  RemoteCopy-Setup-{version}-win64.exe
$nsisPattern = Join-Path $TauriDir "target\release\bundle\nsis\*-setup.exe"
$nsisSource  = Get-Item $nsisPattern -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($nsisSource) {
    $dest = Join-Path $OutDir "RemoteCopy-Setup-$Version-win64.exe"
    Copy-Item $nsisSource.FullName $dest -Force
    Write-Host "    $($nsisSource.Name)  →  $(Split-Path $dest -Leaf)" -ForegroundColor Gray
} else {
    Write-Warning "NSIS installer not found — check Tauri build output."
}

# Portable exe  →  RemoteCopy-Portable-{version}-win64.exe
$portableSource = Join-Path $TauriDir "target\release\remotecopy-gui.exe"
if (Test-Path $portableSource) {
    $dest = Join-Path $OutDir "RemoteCopy-Portable-$Version-win64.exe"
    Copy-Item $portableSource $dest -Force
    Write-Host "    remotecopy-gui.exe  →  $(Split-Path $dest -Leaf)" -ForegroundColor Gray
} else {
    Write-Warning "Portable exe not found at $portableSource"
}

Write-Host "    Done." -ForegroundColor Green

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "Windows artifacts ready in: release-artifacts\" -ForegroundColor Green
Write-Host ""
Write-Host "Tag and release:" -ForegroundColor Cyan
Write-Host "  git add -A" -ForegroundColor White
Write-Host "  git commit -m `"chore: bump version to $Version`"" -ForegroundColor White
Write-Host "  git tag v$Version" -ForegroundColor White
Write-Host "  git push origin master --tags" -ForegroundColor White
Write-Host "  gh release create v$Version release-artifacts/* --title `"v$Version`" --draft" -ForegroundColor White
Write-Host ""
