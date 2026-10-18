@echo off
chcp 65001 >nul
REM 构建 Android 探针 APK（L0 / M0）。脚本位于 scripts/，先切到项目根目录。
pushd "%~dp0.." >nul

if "%~1"=="" ( set "ABI=arm64-v8a" ) else ( set "ABI=%~1" )
if "%API%"=="" set "API=28"
if "%PKG%"=="" set "PKG=lumino-android-probe"

if "%ANDROID_HOME%"=="" if not "%ANDROID_SDK_ROOT%"=="" set "ANDROID_HOME=%ANDROID_SDK_ROOT%"
if "%ANDROID_HOME%"=="" (
    echo [错误] 未设置 ANDROID_HOME / ANDROID_SDK_ROOT
    popd >nul
    exit /b 1
)

cargo ndk --version >nul 2>&1
if errorlevel 1 (
    echo [错误] 缺少 cargo-ndk，请先: cargo install cargo-ndk
    popd >nul
    exit /b 1
)

echo [1/2] 交叉编译 %PKG% -^> %ABI% ^(API %API%^)
cargo ndk -t %ABI% -P %API% -o android\app\src\main\jniLibs build -p %PKG% --release
if errorlevel 1 (
    echo [错误] 交叉编译失败
    popd >nul
    exit /b 1
)

echo [2/2] 打包 APK
pushd android >nul
call gradlew.bat --no-daemon assembleDebug
set "RC=%errorlevel%"
popd >nul
if not "%RC%"=="0" (
    echo [错误] Gradle 打包失败
    popd >nul
    exit /b %RC%
)

echo.
echo [完成] APK: android\app\build\outputs\apk\debug\app-debug.apk
echo [提示] 安装: adb install -r android\app\build\outputs\apk\debug\app-debug.apk
echo [提示] 日志: adb logcat -s lumino-probe RustStdoutStderr
popd >nul
