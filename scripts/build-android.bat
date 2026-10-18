@echo off
REM Build the Android probe APK (L0 / M0). Script lives in scripts/; switch to repo root.
REM NOTE: keep this file ASCII-only -- cmd.exe mis-parses UTF-8 Chinese in echo lines.
pushd "%~dp0.." >nul

if "%~1"=="" ( set "ABI=arm64-v8a" ) else ( set "ABI=%~1" )
if "%API%"=="" set "API=28"
if "%PKG%"=="" set "PKG=lumino-android-probe"

if "%ANDROID_HOME%"=="" if not "%ANDROID_SDK_ROOT%"=="" set "ANDROID_HOME=%ANDROID_SDK_ROOT%"
if "%ANDROID_HOME%"=="" (
    echo [ERROR] ANDROID_HOME / ANDROID_SDK_ROOT is not set
    popd >nul
    exit /b 1
)

cargo ndk --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] cargo-ndk not found. Run: cargo install cargo-ndk
    popd >nul
    exit /b 1
)

echo [1/2] cross-compiling %PKG% for %ABI% ^(API %API%^)
cargo ndk -t %ABI% -P %API% -o android\app\src\main\jniLibs build -p %PKG% --release
if errorlevel 1 (
    echo [ERROR] cross compilation failed
    popd >nul
    exit /b 1
)

echo [2/2] packaging APK
pushd android >nul
call gradlew.bat --no-daemon assembleDebug
set "RC=%errorlevel%"
popd >nul
if not "%RC%"=="0" (
    echo [ERROR] gradle build failed
    popd >nul
    exit /b %RC%
)

echo.
echo [DONE] APK: android\app\build\outputs\apk\debug\app-debug.apk
echo [HINT] install: adb install -r android\app\build\outputs\apk\debug\app-debug.apk
echo [HINT] logs:    adb logcat -s lumino-probe RustStdoutStderr
popd >nul
