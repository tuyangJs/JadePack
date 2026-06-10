param(
    [string]$Password = "jadepack2026",
    [string]$JadeTweakPublic = "d:\nodejsApp\JadeTweak\public\downloads\jadepack"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host " JadePack Release: Build + Sign + Deploy" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$tauriConf = Get-Content "$Root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$version = $tauriConf.version
Write-Host "`n[1/4] Version: $version" -ForegroundColor Yellow

Write-Host "`n[2/4] Running bun run tauri build ..." -ForegroundColor Yellow
Push-Location $Root
try {
    bun run tauri build
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri build failed, exit code: $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
Write-Host "  Build complete" -ForegroundColor Green

$nsisDir = "$Root\src-tauri\target\release\bundle\nsis"
$installer = Get-ChildItem -Path $nsisDir -Filter "JadePack_*_x64-setup.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1

if (-not $installer) {
    throw "Installer not found: $nsisDir\JadePack_*_x64-setup.exe"
}

$others = Get-ChildItem -Path $nsisDir -Filter "JadePack_*_x64-setup.exe" | Where-Object { $_.Name -ne $installer.Name }
$others | ForEach-Object { Remove-Item $_.FullName -Force; if (Test-Path "$($_.FullName).sig") { Remove-Item "$($_.FullName).sig" -Force } }

Write-Host "`n[3/4] Signing: $($installer.Name)" -ForegroundColor Yellow
Push-Location $Root
try {
    bunx @tauri-apps/cli signer sign `
        --private-key-path ./update_keys `
        --password $Password `
        $installer.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "Sign failed, exit code: $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
Write-Host "  Sign complete" -ForegroundColor Green

Write-Host "`n[4/4] Deploy to JadeTweak: $JadeTweakPublic" -ForegroundColor Yellow

if (-not (Test-Path $JadeTweakPublic)) {
    New-Item -ItemType Directory -Path $JadeTweakPublic -Force | Out-Null
} else {
    Get-ChildItem -Path $JadeTweakPublic -Filter "JadePack_*_x64-setup.exe" | Remove-Item -Force
    Get-ChildItem -Path $JadeTweakPublic -Filter "JadePack_*_x64-setup.exe.sig" | Remove-Item -Force
}

Copy-Item -Path $installer.FullName -Destination $JadeTweakPublic -Force
Copy-Item -Path "$($installer.FullName).sig" -Destination $JadeTweakPublic -Force

Write-Host "  Deployed:" -ForegroundColor Green
Write-Host "    $JadeTweakPublic\$($installer.Name)" -ForegroundColor Green
Write-Host "    $JadeTweakPublic\$($installer.Name).sig" -ForegroundColor Green

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host " Release complete! Version: v$version" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan