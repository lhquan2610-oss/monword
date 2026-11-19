$ErrorActionPreference = "Continue"

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$WinLibsBin = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs*\mingw64\bin" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
if ($WinLibsBin) {
    $env:PATH = "$WinLibsBin;$env:PATH"
}

Write-Host "==> [1/4] Building Frontend (ui/dist)..." -ForegroundColor Cyan
Push-Location (Join-Path $Root "ui")
npm run build
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
Pop-Location

Write-Host "==> [2/4] Running Rust Integration Tests..." -ForegroundColor Cyan
Push-Location $Root
cargo test
if ($LASTEXITCODE -ne 0) { throw "Cargo test failed" }

Write-Host "==> [3/4] Building Release Binary (x86_64-pc-windows-gnullvm)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed" }
Pop-Location

Write-Host "==> [4/4] Packaging dist-portable..." -ForegroundColor Cyan
$ReleaseDir = Join-Path $Root "target\x86_64-pc-windows-gnullvm\release"
$DistDir = Join-Path $Root "dist-portable"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

# Clean up legacy filename from before MonWord rebranding
Remove-Item -Path (Join-Path $DistDir "ChuanHoaVanBanPro.exe") -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $DistDir "ChuanHoaVanBanPro.exe.WebView2") -Recurse -Force -ErrorAction SilentlyContinue

$TargetExe = Join-Path $DistDir "MonWord.exe"
try {
    Copy-Item (Join-Path $ReleaseDir "chuanhoa-vb-pro.exe") $TargetExe -Force -ErrorAction Stop
} catch {
    $FallbackExe = Join-Path $DistDir "MonWord-v1.0.0.exe"
    Write-Warning "MonWord.exe đang chạy và bị khóa; đóng gói bản mới sang MonWord-v1.0.0.exe."
    Copy-Item (Join-Path $ReleaseDir "chuanhoa-vb-pro.exe") $FallbackExe -Force -ErrorAction Stop
}

$LoaderDll = Join-Path $ReleaseDir "WebView2Loader.dll"
if (-not (Test-Path $LoaderDll)) {
    throw "Missing WebView2Loader.dll in $ReleaseDir"
}
$TargetDll = Join-Path $DistDir "WebView2Loader.dll"
try {
    Copy-Item $LoaderDll $TargetDll -Force -ErrorAction Stop
} catch {
    if (-not (Test-Path $TargetDll)) {
        throw
    }
    Write-Warning "WebView2Loader.dll đang được dùng; giữ nguyên DLL hiện có tại $TargetDll."
}

Write-Host "Hoàn tất đóng gói tại: $DistDir" -ForegroundColor Green
Get-ChildItem $DistDir | Format-Table Name, Length, LastWriteTime
