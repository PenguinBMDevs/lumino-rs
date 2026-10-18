#!/usr/bin/env bash
# 构建 Android 探针 APK（L0 / M0）。脚本位于 scripts/，先切到项目根目录。
set -euo pipefail
cd "$(dirname "$0")/.."

ABI="${1:-arm64-v8a}"
API="${API:-28}"
PKG="${PKG:-lumino-android-probe}"

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-}}"
if [ -z "$ANDROID_HOME" ]; then
    echo "[错误] 未设置 ANDROID_HOME / ANDROID_SDK_ROOT" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "[错误] 缺少 cargo" >&2
    exit 1
fi

if ! cargo ndk --version >/dev/null 2>&1; then
    echo "[错误] 缺少 cargo-ndk，请先: cargo install cargo-ndk" >&2
    exit 1
fi

echo "[1/2] 交叉编译 $PKG -> $ABI (API $API)"
cargo ndk -t "$ABI" -P "$API" -o android/app/src/main/jniLibs build -p "$PKG" --release

echo "[2/2] 打包 APK"
(cd android && ./gradlew --no-daemon assembleDebug)

echo
echo "[完成] APK: android/app/build/outputs/apk/debug/app-debug.apk"
echo "[提示] 安装: adb install -r android/app/build/outputs/apk/debug/app-debug.apk"
echo "[提示] 日志: adb logcat -s lumino-probe RustStdoutStderr"
