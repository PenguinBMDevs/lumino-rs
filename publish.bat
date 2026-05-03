@echo off
chcp 65001 > nul
REM ============================================================
REM Lumino Release Build Script for Windows
REM Builds release binaries for Windows platforms.
REM
REM Run on: Windows 10/11 with Rust installed
REM ============================================================

setlocal EnableDelayedExpansion

set "PROJECT_DIR=%~dp0"
set "PUBLISH_DIR=%PROJECT_DIR%publish"

echo [INFO] Lumino Windows Release Builder
echo.

REM Check if rustup is available
where rustup > nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [ERR] rustup not found. Please install Rust first:
    echo       https://rustup.rs/
    exit /b 1
)

REM Install Windows targets if not present
echo [INFO] Checking Rust targets...
rustup target list --installed | findstr "x86_64-pc-windows-msvc" > nul
if %ERRORLEVEL% neq 0 (
    echo [INFO] Installing x86_64-pc-windows-msvc target...
    rustup target add x86_64-pc-windows-msvc
)

rustup target list --installed | findstr "aarch64-pc-windows-msvc" > nul
if %ERRORLEVEL% neq 0 (
    echo [INFO] Installing aarch64-pc-windows-msvc target...
    rustup target add aarch64-pc-windows-msvc
)

echo [OK] Rust targets ready
echo.

REM ============================================================
REM Build: x86_64 Windows
REM ============================================================
echo [INFO] Building x86_64 Windows (native)...
cd /d "%PROJECT_DIR%"
cargo build --release

if %ERRORLEVEL% neq 0 (
    echo [ERR] x86_64 Windows build failed
    exit /b 1
)

if not exist "%PUBLISH_DIR%\windows-amd64" mkdir "%PUBLISH_DIR%\windows-amd64"
copy /y "target\release\lumino-rs.exe" "%PUBLISH_DIR%\windows-amd64\" > nul
echo [OK] windows-amd64 built
echo.

REM ============================================================
REM Build: aarch64 Windows
REM ============================================================
echo [INFO] Building aarch64 Windows (cross-compile)...
cargo build --release --target aarch64-pc-windows-msvc

if %ERRORLEVEL% neq 0 (
    echo [WARN] aarch64 Windows build failed (this is normal if MSVC arm64 toolchain is not installed)
    echo [WARN] Skipping windows-arm64 build
) else (
    if not exist "%PUBLISH_DIR%\windows-arm64" mkdir "%PUBLISH_DIR%\windows-arm64"
    copy /y "target\aarch64-pc-windows-msvc\release\lumino-rs.exe" "%PUBLISH_DIR%\windows-arm64\" > nul
    echo [OK] windows-arm64 built
)
echo.

REM ============================================================
REM Summary
REM ============================================================
echo ========================================
echo   Windows builds completed!
echo ========================================
echo.
echo Published artifacts:
for /r "%PUBLISH_DIR%" %%f in (*.exe) do (
    for %%a in ("%%f") do set "size=%%~za"
    echo   %%~nxf (!size! bytes)
)
echo.
echo Output directory: %PUBLISH_DIR%

endlocal
