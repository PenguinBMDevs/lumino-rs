@echo off
chcp 65001 >nul

set "MODE=%~1"

if "%~1"=="" set "MODE=release"

if /I "%MODE%"=="release" (
    echo Building in release mode...
    cargo build --release
    if errorlevel 1 (
        echo Build failed!
        exit /b 1
    )
    set "SRC=target\release\lumino-rs.exe"
) else if /I "%MODE%"=="debug" (
    echo Building in debug mode...
    cargo build
    if errorlevel 1 (
        echo Build failed!
        exit /b 1
    )
    set "SRC=target\debug\lumino-rs.exe"
) else if /I "%MODE%"=="fast-release" (
    echo Building in fast-release mode (console + debug symbols)...
    cargo build --profile fast-release
    if errorlevel 1 (
        echo Build failed!
        exit /b 1
    )
    set "SRC=target\fast-release\lumino-rs.exe"
) else (
    echo Invalid mode. Usage: %~nx0 [release^|debug^|fast-release]
    echo   release      - 正式发布模式（strip, LTO, 无调试信息）
    echo   debug        - 开发调试模式（增量编译, 完整调试信息）
    echo   fast-release - 快速发布+调试（有限优化, 行号表, Windows终端显示）
    echo Default is release mode.
    exit /b 1
)

if not exist "bin" mkdir "bin"

copy /Y "%SRC%" "bin\lumino-rs.exe" >nul
if errorlevel 1 (
    echo Failed to copy executable!
    exit /b 1
)

echo Build completed successfully!
echo Executable copied to: bin\lumino-rs.exe
