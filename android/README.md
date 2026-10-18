# Lumino Android 壳工程

本目录是 lumino-rs 的 Android 打包壳。当前承载 **L0 / M0 探针**
（`crates/tools/android-probe`），后续正式版（`lumino-rs` 主程序）复用同一套壳。

## 前置条件

| 依赖 | 要求 | 说明 |
|---|---|---|
| JDK | 17 或 21 | `JAVA_HOME` 指向 JDK，Gradle 用 |
| Android SDK | `platforms/android-33` | `ANDROID_HOME` / `ANDROID_SDK_ROOT` 指向 SDK |
| Android NDK | r26+（本机 r30.0.14904198） | Rust 交叉编译需要 clang 与 ar |
| `cargo-ndk` | 4.x | `cargo install cargo-ndk` |
| Rust target | `aarch64-linux-android` | `rustup target add aarch64-linux-android` |

`local.properties`（`sdk.dir=...`）**不入库**，AGP 也可直接读 `ANDROID_HOME`。

## 构建

```bash
# Windows
scripts\build-android.bat            # 默认 arm64-v8a
# Linux / macOS
./scripts/build-android.sh

# 等价的手动两步
cargo ndk -t arm64-v8a -P 28 -o android/app/src/main/jniLibs build -p lumino-android-probe --release
cd android && ./gradlew --no-daemon assembleDebug
```

产物：`android/app/build/outputs/apk/debug/app-debug.apk`

## 安装与看日志

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb logcat -s lumino-probe            # 探针自己的日志（android_logger tag）
adb logcat -s RustStdoutStderr        # Rust panic / stdio
```

首屏应显示：「Lumino Android 探针 / 中文 English 混排 / 计数 = 0 / 滑杆 / 输入框」。
点 `+1` 能加一 → 触摸→点击链路通；拖滑杆 → 连续触摸通；点输入框 → 软键盘弹起
（`set_ime_allowed(true)`）；中文不是豆腐块 → 内置子集字体 + `/system/fonts` 回退通。

## 关键约定（改之前先读）

1. **`compileSdk 33`**：本机 SDK 只装了 `android-30/33/36.1`，选 33 可完全离线构建。
   因此 `androidx.games:games-activity` 固定 **1.2.2**（2.x 要求 compileSdk 34+）。
2. **必须带 `androidx.appcompat`**：`GameActivity` 继承 `AppCompatActivity`，缺它 javac
   会报「无法访问 AppCompatActivity」，并连带所有继承方法（`getWindow()` 等）全部失踪；
   主题必须是 `Theme.AppCompat` 派生（见 `res/values/themes.xml`）。
3. **原生库名三处一致**：`cargo [lib] name = "lumino_probe"` ⇄
   `AndroidManifest` 的 `android.app.lib_name` ⇄ `MainActivity` 的
   `System.loadLibrary("lumino_probe")`。
4. **jniLibs 不入库**：`.so` 由 `cargo ndk -o app/src/main/jniLibs` 产出，已在
   `.gitignore` 排除。
5. **不做 C/C++ 编译**：没有 CMake/ndk-build，故 `build.gradle` 不声明 `ndkVersion`。

## 目录

```
android/
├── app/
│   ├── build.gradle           # compileSdk 33 / minSdk 28 / arm64-v8a
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── java/dev/penguinbm/lumino/MainActivity.java
│       └── res/values/{strings,themes}.xml
├── gradle/wrapper/            # Gradle 8.10.2
├── build.gradle               # AGP 8.8.0
├── settings.gradle
└── gradle.properties
```
