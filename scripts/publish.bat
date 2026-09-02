@echo off
chcp 65001 > nul
setlocal EnableDelayedExpansion

REM ============================================================
REM Lumino Release Build Script for Windows
REM Builds release binaries for Windows platforms.
REM
REM Run on: Windows 10/11 with Rust installed
REM ============================================================

set "PROJECT_DIR=%~dp0.."
REM 去掉尾部斜杠并解析为绝对路径
for %%I in ("%PROJECT_DIR%") do set "PROJECT_DIR=%%~fI\"
set "PUBLISH_DIR=%PROJECT_DIR%publish"

REM 日志函数
:log_info
echo [INFO] %~1
goto :eof

:log_ok
echo [OK] %~1
goto :eof

:log_warn
echo [WARN] %~1
goto :eof

:log_err
echo [ERR] %~1
goto :eof

:main

REM Check if rustup is available
where rustup > nul 2>&1
if %ERRORLEVEL% neq 0 (
    call :log_err "rustup not found. Please install Rust first: https://rustup.rs/"
    exit /b 1
)

REM Install Windows targets if not present
call :log_info "Checking Rust targets..."
rustup target list --installed | findstr "x86_64-pc-windows-msvc" > nul
if %ERRORLEVEL% neq 0 (
    call :log_info "Installing x86_64-pc-windows-msvc target..."
    rustup target add x86_64-pc-windows-msvc
)

rustup target list --installed | findstr "aarch64-pc-windows-msvc" > nul
if %ERRORLEVEL% neq 0 (
    call :log_info "Installing aarch64-pc-windows-msvc target..."
    rustup target add aarch64-pc-windows-msvc
)

call :log_ok "Rust targets ready"
echo.

REM ============================================================
REM Build: x86_64 Windows
REM ============================================================
call :log_info "Building x86_64 Windows (native)..."
cd /d "%PROJECT_DIR%"
cargo build --release

if %ERRORLEVEL% neq 0 (
    call :log_err "x86_64 Windows build failed"
    exit /b 1
)

if not exist "%PUBLISH_DIR%\windows-amd64" mkdir "%PUBLISH_DIR%\windows-amd64"
copy /y "target\release\lumino-rs.exe" "%PUBLISH_DIR%\windows-amd64\" > nul
call :log_ok "windows-amd64 built"
echo.

REM ============================================================
REM Build: aarch64 Windows
REM ============================================================
call :log_info "Building aarch64 Windows (cross-compile)..."
cargo build --release --target aarch64-pc-windows-msvc

if %ERRORLEVEL% neq 0 (
    call :log_warn "aarch64 Windows build failed (this is normal if MSVC arm64 toolchain is not installed)"
    call :log_warn "Skipping windows-arm64 build"
) else (
    if not exist "%PUBLISH_DIR%\windows-arm64" mkdir "%PUBLISH_DIR%\windows-arm64"
    copy /y "target\aarch64-pc-windows-msvc\release\lumino-rs.exe" "%PUBLISH_DIR%\windows-arm64\" > nul
    call :log_ok "windows-arm64 built"
)
echo.

REM ============================================================
REM Summary
REM ============================================================
call :log_ok "Windows builds completed!"
echo.
call :log_info "Published artifacts:"
for /r "%PUBLISH_DIR%" %%f in (*.exe) do (
    for %%a in ("%%f") do set "size=%%~za"
    echo   %%~nxf (!size! bytes)
)
echo.
call :log_info "Output directory: %PUBLISH_DIR%"

endlocal
