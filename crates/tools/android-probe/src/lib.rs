//! Lumino Android 探针（L0 / M0）。
//!
//! 只验证四件「不验证就不能动 L1~L4」的前置假设：
//! 1. `android_main` 能被 android-activity 的 `ANativeActivity_onCreate` 拉起；
//! 2. wgpu 27 在设备 Vulkan/GLES 上能建 surface 并出画面；
//! 3. iced 0.14（fork 分支 `lumino-android-0.14`）能渲染中文——子集化
//!    Noto Sans SC 由 `include_bytes!` 打进 `.so`，经 `font_system().load_font()` 注册；
//! 4. 触摸能转成 iced 光标/点击，`set_ime_allowed(true)` 能拉起软键盘。
//!
//! 桌面构建只保证编译通过（cdylib 无入口），真验证必须在 Android 设备上看 logcat。

mod app;
mod font;
mod ui;

pub use app::App;
pub use ui::{Message, Probe};

/// Android 入口：符号名固定为 `android_main`，由 android-activity 胶水调用。
///
/// edition 2024 下 `#[no_mangle]` 必须写成 `#[unsafe(no_mangle)]`。
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("lumino-probe"),
    );

    log::info!("android_main 启动（Lumino Android 探针 M0）");

    let event_loop = match winit::event_loop::EventLoop::<()>::with_user_event()
        .with_android_app(android_app)
        .build()
    {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log::error!("创建 android 事件循环失败: {error}");
            return;
        }
    };

    let mut app = App::new();

    if let Err(error) = event_loop.run_app(&mut app) {
        log::error!("事件循环异常退出: {error}");
    }
}
