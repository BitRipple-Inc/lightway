param(
    [ValidateSet("x86_64", "arm64")]
    [string]$Arch = "x86_64",

    # Root of the pre-built dependency tree produced by AxlWinBuild.
    # Must contain:
    #   $InstallDir\llvm\          -- host LLVM (x64 build tools, needed by bindgen)
    #   $InstallDir\llvm_target\   -- target LLVM headers (only needed for ARM64 cross-compile)
    #   $InstallDir\include\       -- AXL + csnip headers
    #   $InstallDir\lib\           -- AXL + csnip static .lib files
    [Parameter(Mandatory=$true)]
    [string]$InstallDir,

    # Override target LLVM path (defaults to $InstallDir\llvm for x86_64,
    # and $InstallDir\llvm_target for arm64).
    [string]$TargetLlvmPath = ""
)

$ErrorActionPreference = "Stop"

# --- Helpers ---
function Write-Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Step($msg) { Write-Host "`n--- $msg ---" }

# --- Target triple mapping ---
$TargetMap = @{
    "x86_64" = "x86_64-pc-windows-msvc"
    "arm64"  = "aarch64-pc-windows-msvc"
}
$Target = $TargetMap[$Arch]

# --- Resolve paths ---
# The script lives in scripts/ inside the repo root.
$LightwayDir = Split-Path -Parent $PSScriptRoot
$Earthfile   = Join-Path $LightwayDir "Earthfile"

# Default target LLVM: same as host for x86_64, separate tree for arm64
if ($TargetLlvmPath -eq "") {
    if ($Arch -eq "arm64") {
        $TargetLlvmPath = Join-Path $InstallDir "llvm_target"
    } else {
        $TargetLlvmPath = Join-Path $InstallDir "llvm"
    }
}

$HostLlvmPath = Join-Path $InstallDir "llvm"

# --- Derive Rust version from Earthfile ---
# Mirrors: grep "FROM rust" Earthfile | awk -F'[:\-]' '{print $2}'
$EarthfileContent = Get-Content $Earthfile -Raw
if ($EarthfileContent -match 'FROM rust:(\d+\.\d+\.\d+)') {
    $RustVersion = $Matches[1]
} else {
    Write-Error "Could not determine Rust version from $Earthfile"
    exit 1
}

Write-Info "Rust version    : $RustVersion"
Write-Info "Target          : $Target"
Write-Info "InstallDir      : $InstallDir"
Write-Info "Host LLVM       : $HostLlvmPath"
Write-Info "Target LLVM     : $TargetLlvmPath"

# --- Verify prerequisites ---
if (-not (Test-Path $HostLlvmPath)) {
    Write-Error "Host LLVM not found at '$HostLlvmPath'. Install LLVM there or pass -TargetLlvmPath."
}
if (-not (Test-Path $TargetLlvmPath)) {
    Write-Error "Target LLVM not found at '$TargetLlvmPath'."
}
foreach ($folder in @("include", "lib")) {
    $dest = Join-Path $InstallDir $folder
    if (-not (Test-Path $dest)) {
        Write-Error "Required C library directory missing: '$dest'. Run AxlWinBuild first."
    }
}

# --- Set environment variables for lt3_plugin/build.rs ---
# bindgen needs the host LLVM to run on the build machine
$env:LIBCLANG_PATH = Join-Path $HostLlvmPath "bin"
$env:CLANG_PATH    = Join-Path $HostLlvmPath "bin\clang.exe"

# bindgen clang args: target triple + target LLVM headers (ARM64-correct type widths)
$TargetLlvmInclude = Join-Path $TargetLlvmPath "include"
$env:BINDGEN_EXTRA_CLANG_ARGS = "--target=$Target -I`"$TargetLlvmInclude`" -I`"$InstallDir\include`" -I`"$InstallDir\include\csnip-0.5`""

# AXL + csnip header locations (read by lt3_plugin/build.rs on Windows)
$env:AXL_INCLUDE_DIR   = Join-Path $InstallDir "include"
$env:CSNIP_INCLUDE_DIR = Join-Path $InstallDir "include\csnip-0.5"

# AXL + csnip static .lib locations
$env:AXL_LIB_DIR   = Join-Path $InstallDir "lib"
$env:CSNIP_LIB_DIR = Join-Path $InstallDir "lib"

# pkg-config cross-compile settings (required even on Windows for the build script)
$env:PKG_CONFIG_ALLOW_CROSS   = "1"
$env:PKG_CONFIG_PATH          = Join-Path $InstallDir "lib\pkgconfig"
$env:PKG_CONFIG_SYSROOT_DIR   = $InstallDir

Write-Info "Environment configured for lt3_plugin C dependencies."

# --- Install Rust toolchain and build ---
Push-Location $LightwayDir
try {
    rustup toolchain install $RustVersion
    rustup target add $Target --toolchain $RustVersion
    rustup component add clippy rustfmt --toolchain $RustVersion

    $CargoArgs = @("+$RustVersion")

    # Check formatting
    Write-Step "cargo fmt --check"
    cargo @CargoArgs fmt --check

    # Run clippy
    Write-Step "cargo clippy"
    cargo @CargoArgs clippy --package lightway-client --target $Target -- -D warnings

    # Build release binary
    Write-Step "cargo build --release"
    cargo @CargoArgs build --release --bin lightway-client --target $Target

    # Run tests
    Write-Step "cargo test"
    cargo @CargoArgs test --target $Target -p lightway-client

    $Artifact = Join-Path $LightwayDir "target\$Target\release\lightway-client.exe"
    Write-Host "`nBuild complete: $Artifact"
    Write-Host ""
    Write-Host "RUNTIME REQUIREMENT: wintun.dll" -ForegroundColor Yellow
    Write-Host "  lightway-client.exe uses tun-rs to create a virtual TUN adapter on Windows."
    Write-Host "  tun-rs loads wintun.dll dynamically at runtime -- it cannot be statically linked."
    Write-Host "  Without it the client will fail immediately with: LoadLibraryExW failed (os error 126)"
    Write-Host ""
    Write-Host "  To install wintun.dll:"
    Write-Host "    1. Download the Wintun zip from https://wintun.net/"
    Write-Host "    2. Extract the zip -- it contains binaries for amd64, arm64, x86, arm"
    Write-Host "    3. Copy the correct wintun.dll next to lightway-client.exe:"
    if ($Arch -eq "arm64") {
        Write-Host "         arm64\wintun.dll  ->  $(Split-Path $Artifact)"
    } else {
        Write-Host "         amd64\wintun.dll  ->  $(Split-Path $Artifact)"
    }
    Write-Host "    4. Run lightway-client.exe as Administrator (required to create TUN adapters)"
} finally {
    Pop-Location
}
