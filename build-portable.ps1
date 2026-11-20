$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$WinLibsBin = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs*\mingw64\bin" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
if ($WinLibsBin) {
    $env:PATH = "$WinLibsBin;$env:PATH"
}

$Version = "4.0.0"

Write-Host "==> [1/5] Building Frontend (ui/dist)..." -ForegroundColor Cyan
Push-Location (Join-Path $Root "ui")
npm run build
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
Pop-Location

Write-Host "==> [2/5] Running Rust Integration & Release Tests..." -ForegroundColor Cyan
Push-Location $Root
cargo test
if ($LASTEXITCODE -ne 0) { throw "Cargo test failed" }

Write-Host "==> [3/5] Building Release Binaries (x86_64-pc-windows-gnullvm)..." -ForegroundColor Cyan
cargo build --release --bin chuanhoa-vb-pro --bin monword-installer
if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed" }
Pop-Location

Write-Host "==> [4/5] Packaging dist-portable..." -ForegroundColor Cyan
$ReleaseDir = Join-Path $Root "target\x86_64-pc-windows-gnullvm\release"
$DistDir = Join-Path $Root "dist-portable"
$DistDocsDir = Join-Path $DistDir "docs"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
New-Item -ItemType Directory -Force -Path $DistDocsDir | Out-Null

# Clean up legacy filenames from previous builds
Remove-Item -Path (Join-Path $DistDir "ChuanHoaVanBanPro.exe") -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $DistDir "ChuanHoaVanBanPro.exe.WebView2") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $DistDir "MonWord-v1.0.0.exe") -Force -ErrorAction SilentlyContinue

$TargetExe = Join-Path $DistDir "MonWord.exe"
try {
    Copy-Item (Join-Path $ReleaseDir "chuanhoa-vb-pro.exe") $TargetExe -Force -ErrorAction Stop
} catch {
    $FallbackExe = Join-Path $DistDir "MonWord-v$Version.exe"
    Write-Warning "MonWord.exe đang chạy và bị khóa; đóng gói bản mới sang MonWord-v$Version.exe."
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

Copy-Item (Join-Path $Root "assets\monword.ico") (Join-Path $DistDir "monword.ico") -Force
Copy-Item (Join-Path $Root "README.md") (Join-Path $DistDir "README.md") -Force
Copy-Item (Join-Path $Root "docs\*.md") $DistDocsDir -Force

Write-Host "==> [5/5] Packaging dist-release (Setup Installer + Portable ZIP + SHA256SUMS)..." -ForegroundColor Cyan
$OutReleaseDir = Join-Path $Root "dist-release"
New-Item -ItemType Directory -Force -Path $OutReleaseDir | Out-Null

$SetupExe = Join-Path $OutReleaseDir "MonWord-Setup-v$Version-x64.exe"
$StubExe = Join-Path $ReleaseDir "monword-installer.exe"
$PackProc = Start-Process -FilePath $StubExe -ArgumentList @("--pack-sfx", "--stub", "`"$StubExe`"", "--dir", "`"$DistDir`"", "--out", "`"$SetupExe`"") -Wait -NoNewWindow -PassThru
if ($PackProc.ExitCode -ne 0) { throw "Packing self-extracting setup failed with exit code $($PackProc.ExitCode)" }
if (-not (Test-Path $SetupExe)) { throw "Missing Setup EXE output: $SetupExe" }

$PortableZip = Join-Path $OutReleaseDir "MonWord-v$Version-Portable-x64.zip"
Remove-Item -Path $PortableZip -Force -ErrorAction SilentlyContinue
$ZipStaging = Join-Path $env:TEMP ("monword_zip_stage_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $ZipStaging | Out-Null
try {
    $StageFiles = @("MonWord.exe", "WebView2Loader.dll", "monword.ico", "README.md")
    foreach ($f in $StageFiles) {
        $srcF = Join-Path $DistDir $f
        if (Test-Path $srcF) { Copy-Item $srcF (Join-Path $ZipStaging $f) -Force }
    }
    $StageDocs = Join-Path $ZipStaging "docs"
    New-Item -ItemType Directory -Force -Path $StageDocs | Out-Null
    Copy-Item (Join-Path $DistDocsDir "*.md") $StageDocs -Force
    Compress-Archive -Path (Join-Path $ZipStaging "*") -DestinationPath $PortableZip -CompressionLevel Optimal -Force
} finally {
    Remove-Item -Path $ZipStaging -Recurse -Force -ErrorAction SilentlyContinue
}

$SumsFile = Join-Path $OutReleaseDir "SHA256SUMS.txt"
$HashLines = @()
foreach ($artifact in @($SetupExe, $PortableZip, $TargetExe, $TargetDll)) {
    if (Test-Path $artifact) {
        $h = Get-FileHash -Path $artifact -Algorithm SHA256
        $name = Split-Path -Leaf $artifact
        $HashLines += "$($h.Hash.ToLowerInvariant())  $name"
    }
}
Set-Content -Path $SumsFile -Value $HashLines -Encoding UTF8

Write-Host "Hoàn tất đóng gói portable tại: $DistDir" -ForegroundColor Green
Get-ChildItem $DistDir | Format-Table Name, Length, LastWriteTime

Write-Host "Hoàn tất phát hành release tại: $OutReleaseDir" -ForegroundColor Green
Get-ChildItem $OutReleaseDir | Format-Table Name, Length, LastWriteTime
