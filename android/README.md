# Lumino Android 壳工程

本目录是 lumino-rs 的 Android 打包壳。当前承载 **L0 / M0 探针**
（`crates/tools/android-probe`），后续正式版（`lumino-rs` 主程序）复用同一套壳。

## 前置条件

| 依赖 | 要求 | 说明 |
|---|---|---|
| JDK | 17 或 21 | `JAVA_HOME` 指向 JDK，Gradle 用 |
| Android SDK | `platforms/android-34` | `ANDROID_HOME` / `ANDROID_SDK_ROOT` 指向 SDK |
| Android NDK | r26+（本机 r30.0.14904198） | Rust 交叉编译需要 clang 与 ar |
| `cargo-ndk` | 4.x | `cargo install cargo-ndk` |
| Rust target | `aarch64-linux-android` + `x86_64-linux-android` | `rustup target add aarch64-linux-android x86_64-linux-android`（后者给模拟器） |

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

1. **`games-activity` 版本强绑定 `3.0.5`，别乱升降**：Java 侧的 `GameActivity` 必须与
   「`android-activity 0.6.1` 自带的 AGDK C++ glue」的 `RegisterNatives` 表签名逐一对上：
   - glue 要求 `initializeNativeCode(String,String,String,AssetManager,byte[],Configuration)J`
     与 `onTouchEventNative(J,MotionEvent,IIIIIJJIIIIIIFF)Z`；
   - `1.2.2` 连 `initializeNativeCode` 都少参数；`2.0.2` 的 Java 侧 `onTouchEventNative`
     只有 `(J,MotionEvent)` —— 两者都会在启动瞬间
     `SIGABRT: RegisterNatives failed for 'com/google/androidgamesdk/GameActivity'`；
   - `3.0.x` / `4.x` 签名才一致（`javap` 逐版本核对过，3.0.5 实测可跑）。
2. **`compileSdk 34`**：`3.0.5` 与 appcompat 的要求；需
   `sdkmanager "platforms;android-34"`。`targetSdk 34`。
3. **必须带 `androidx.appcompat`**：`GameActivity` 继承 `AppCompatActivity`，缺它 javac
   会报「无法访问 AppCompatActivity」，并连带所有继承方法（`getWindow()` 等）全部失踪；
   主题必须是 `Theme.AppCompat` 派生（见 `res/values/themes.xml`）。
4. **原生库名三处一致**：`cargo [lib] name = "lumino_probe"` ⇄
   `AndroidManifest` 的 `android.app.lib_name` ⇄ `MainActivity` 的
   `System.loadLibrary("lumino_probe")`。
5. **jniLibs 不入库**：`.so` 由 `cargo ndk -o app/src/main/jniLibs` 产出，已在
   `.gitignore` 排除。
6. **不做 C/C++ 编译**：没有 CMake/ndk-build，故 `build.gradle` 不声明 `ndkVersion`。

## 模拟器（已验证：Android 13 / API 33 / x86_64）

宿主为 x86_64，必须额外产出 `x86_64` 的 `.so`（`abiFilters` 已含两种 ABI）。

```bash
# 1) 装系统镜像（约 1.6GB）+ 建 AVD
sdkmanager --install "system-images;android-33;google_apis;x86_64"
avdmanager create avd -n lumino_api33 -k "system-images;android-33;google_apis;x86_64" -d pixel_5

# 2) 无窗口启动（Windows 需 WHPX；-accel-check 确认）
emulator -avd lumino_api33 -no-window -no-audio -no-boot-anim -no-snapshot \
         -gpu swiftshader_indirect -memory 2048 -cores 4

# 3) 等启动 → 装 → 拉日志与截图
adb wait-for-device && adb shell 'while [ "$(getprop sys.boot_completed)" != 1 ]; do sleep 2; done'
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n dev.penguinbm.lumino.probe/dev.penguinbm.lumino.MainActivity
adb logcat -s lumino-probe
adb exec-out screencap -p > screen.png
```

**实测结果（2026-10-18，API 33 / x86_64 / SwiftShader）**：

| 检查项 | 结果 |
|---|---|
| `android_main` 被拉起、进程存活 | ✅ `pidof` 有值 |
| wgpu 建面 | ✅ `surface=1080x2340 scale=2.625 format=Rgba8UnormSrgb adapter="SwiftShader Device"` |
| 中文渲染 | ✅ 内置 Noto Sans SC（2211 KB / 2210 KB）注册成功，截图有字形像素 |
| 触摸→点击 | ✅ 连续 3 次 tap `+1`，计数文本区像素逐次变化（0→1→2→3） |
| 回前台重建 | ✅ HOME 后再进入，`resumed` 重建图形栈并继续渲染 |
| 软键盘/IME | ⚠️ `showSoftInput` 被系统忽略（`InputEnabledSurfaceView ... is not served`），需真机或后续接 GameTextInput 再验 |

## 目录

```
android/
├── app/
│   ├── build.gradle           # compileSdk 34 / minSdk 28 / arm64-v8a + x86_64
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── java/dev/penguinbm/lumino/MainActivity.java
│       └── res/values/{strings,themes}.xml
├── gradle/wrapper/            # Gradle 8.10.2
├── build.gradle               # AGP 8.8.0
├── settings.gradle
└── gradle.properties
```
