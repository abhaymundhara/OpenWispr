# OpenWispr Windows Developer Setup (Hardware Accelerated)
# This script checks for dependencies required for Vulkan GPU acceleration

Write-Host "=== OpenWispr Windows Hardware-Accelerated Setup ===" -ForegroundColor Cyan

# Check for required tools
Write-Host "`nChecking prerequisites..." -ForegroundColor Yellow

# Check Rust
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "[OK] Rust: $(rustc --version)" -ForegroundColor Green
} else {
    Write-Host "[MISSING] Rust - Install from https://rustup.rs" -ForegroundColor Red
}

# Check Node.js
if (Get-Command node -ErrorAction SilentlyContinue) {
    Write-Host "[OK] Node.js: $(node --version)" -ForegroundColor Green
} else {
    Write-Host "[MISSING] Node.js - Install from https://nodejs.org" -ForegroundColor Red
}

# Check LLVM (Clang)
if (Get-Command clang -ErrorAction SilentlyContinue) {
    Write-Host "[OK] LLVM/Clang found." -ForegroundColor Green
} else {
    Write-Host "[MISSING] LLVM/Clang - Required for 'whisper-rs-sys'. Download: https://github.com/llvm/llvm-project/releases" -ForegroundColor Red
}

# Check Vulkan SDK
if ($env:VULKAN_SDK) {
    Write-Host "[OK] Vulkan SDK found: $env:VULKAN_SDK" -ForegroundColor Green
} else {
    Write-Host "[MISSING] Vulkan SDK - Required for GPU acceleration. Download: https://vkr.org/sdk/home/" -ForegroundColor Red
}

Write-Host "`n=== Final Steps ===" -ForegroundColor Cyan
Write-Host "1. Ensure Visual Studio Build Tools 2022 is installed with 'Desktop development with C++'."
Write-Host "2. Restart your terminal after installing dependencies."
Write-Host "3. Run 'pnpm tauri dev' from apps/desktop."

Write-Host "`nSetup check complete!" -ForegroundColor Green
