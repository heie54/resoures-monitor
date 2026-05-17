# Tauri + Vue Resource Monitor Build Script for Windows
# PowerShell build script

param(
    [switch]$Release,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

Write-Host "=============================================" -ForegroundColor Cyan
Write-Host "  Resource Monitor - Tauri Build Script" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host ""

# Set project root
$ProjectRoot = Split-Path -Parent $PSScriptRoot
if (-not $ProjectRoot) {
    $ProjectRoot = Get-Location
}

Write-Host "[1/4] Checking prerequisites..." -ForegroundColor Yellow

# Check Node.js
try {
    $nodeVersion = node --version
    Write-Host "  Node.js: $nodeVersion" -ForegroundColor Green
} catch {
    Write-Host "  ERROR: Node.js is not installed or not in PATH" -ForegroundColor Red
    Write-Host "  Download from: https://nodejs.org/" -ForegroundColor Red
    exit 1
}

# Check npm
try {
    $npmVersion = npm --version
    Write-Host "  npm: v$npmVersion" -ForegroundColor Green
} catch {
    Write-Host "  ERROR: npm is not installed or not in PATH" -ForegroundColor Red
    exit 1
}

# Check Rust
try {
    $rustVersion = rustc --version
    $cargoVersion = cargo --version
    Write-Host "  Rust: $rustVersion" -ForegroundColor Green
    Write-Host "  Cargo: $cargoVersion" -ForegroundColor Green
} catch {
    Write-Host "  ERROR: Rust is not installed or not in PATH" -ForegroundColor Red
    Write-Host "  Install from: https://rustup.rs/" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "[2/4] Cleaning and installing dependencies..." -ForegroundColor Yellow

# Change to project root
Set-Location $ProjectRoot

# Clean if requested
if ($Clean) {
    Write-Host "  Cleaning previous build artifacts..." -ForegroundColor Cyan
    if (Test-Path "node_modules") {
        Remove-Item -Recurse -Force "node_modules"
    }
    if (Test-Path "src-tauri/target") {
        Remove-Item -Recurse -Force "src-tauri/target"
    }
}

# Install npm dependencies
Write-Host "  Installing npm dependencies..." -ForegroundColor Cyan
npm install

if ($LASTEXITCODE -ne 0) {
    Write-Host "  ERROR: npm install failed" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "[3/4] Building Vue frontend..." -ForegroundColor Yellow

# Build Vue frontend
Write-Host "  Running Vite build..." -ForegroundColor Cyan
npm run build

if ($LASTEXITCODE -ne 0) {
    Write-Host "  ERROR: Vite build failed" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "[4/4] Building Tauri application..." -ForegroundColor Yellow

# Build Tauri
if ($Release) {
    Write-Host "  Building release version (optimized)..." -ForegroundColor Cyan
    npm run tauri build
} else {
    Write-Host "  Building development version..." -ForegroundColor Cyan
    npm run tauri build -- --debug
}

if ($LASTEXITCODE -ne 0) {
    Write-Host "  ERROR: Tauri build failed" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "=============================================" -ForegroundColor Green
Write-Host "  Build completed successfully!" -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor Green
Write-Host ""

# Find and display the executable location
$ExePath = Join-Path $ProjectRoot "src-tauri\target\release\resource-monitor.exe"
if (-not (Test-Path $ExePath)) {
    $ExePath = Join-Path $ProjectRoot "src-tauri\target\release\resource-monitor.exe"
}

if (Test-Path $ExePath) {
    Write-Host "Executable location:" -ForegroundColor Cyan
    Write-Host "  $ExePath" -ForegroundColor White
    $fileSize = (Get-Item $ExePath).Length / 1MB
    Write-Host "  Size: $([math]::Round($fileSize, 2)) MB" -ForegroundColor White
} else {
    # Check for debug build
    $DebugExePath = Join-Path $ProjectRoot "src-tauri\target\debug\resource-monitor.exe"
    if (Test-Path $DebugExePath) {
        Write-Host "Debug executable location:" -ForegroundColor Cyan
        Write-Host "  $DebugExePath" -ForegroundColor White
    }
}

Write-Host ""
Write-Host "To run the application:" -ForegroundColor Yellow
Write-Host "  .\src-tauri\target\release\resource-monitor.exe" -ForegroundColor White
Write-Host ""