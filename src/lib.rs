//! lumino-rs 库入口。
//!
//! 桌面 `bin`（`src/main.rs`）与 Android `cdylib`（本文件的 [`android_main`]）共用
//! 同一套启动逻辑：Windows 子系统控制由 `build.rs` 根据 `DEBUG` env 发射 cfg 标记
//! （release → `windows_gui_subsystem`，隐藏终端；debug/fast-release → 控制台）。

use winit::event_loop::{EventLoop, EventLoopBuilder};

/// 全局内存追踪分配器，按子系统统计堆分配。
#[global_allocator]
static GLOBAL_ALLOC: lumino_diagnostics::memtrace::TaggedAlloc =
    lumino_diagnostics::memtrace::TaggedAlloc;

mod cli;
mod constants;
mod device_probe;
mod logging;
mod platform;
mod runner;
mod services;
mod storage;

/// 平台入口句柄。
///
/// - Android：winit 的 `AndroidApp`（`android_main` 的入参，事件循环靠它绑 Looper）；
/// - 桌面：没有对应概念，用 [`std::convert::Infallible`] 占位，永远传 `None`。
#[cfg(target_os = "android")]
pub type PlatformApp = winit::platform::android::activity::AndroidApp;
/// 桌面占位类型（永不构造）。
#[cfg(not(target_os = "android"))]
pub type PlatformApp = std::convert::Infallible;

/// Puffin 服务器包装器，确保在程序结束时才释放。
///
/// Android 上**不启动**：它监听 `0.0.0.0:8585`，在移动端属于「无谓开端口 + 要
/// INTERNET 权限 + 常驻耗电」，且没有 puffin_viewer 可连。
struct PuffinServerHolder {
    #[cfg(not(target_os = "android"))]
    #[allow(dead_code)]
    server: Option<puffin_http::Server>,
}

impl PuffinServerHolder {
    #[cfg(not(target_os = "android"))]
    fn new() -> Self {
        puffin::set_scopes_on(true);
        let server = puffin_http::Server::new("0.0.0.0:8585").ok();
        if server.is_some() {
            tracing::info!("Puffin profiler server started on http://127.0.0.1:8585");
            tracing::info!("Run `cargo install puffin_viewer && puffin_viewer` to view flamegraph");
        } else {
            tracing::warn!("Failed to start Puffin profiler server");
        }
        Self { server }
    }

    #[cfg(target_os = "android")]
    fn new() -> Self {
        tracing::info!("Android：跳过 puffin profiler 服务器（避免监听端口与 INTERNET 权限）");
        Self {}
    }
}

/// 检查 Wayland 合成器是否可用（通过检测 socket 文件是否存在）。
///
/// 仅靠 `WAYLAND_DISPLAY` 环境变量不够可靠——变量可能被设置但合成器未运行。
#[cfg(target_os = "linux")]
fn wayland_compositor_available() -> bool {
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_owned());
    let runtime_dir = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => std::path::Path::new(&dir).to_owned(),
        Err(_) => return false,
    };
    let socket = runtime_dir.join(&display);
    let available = socket.exists();
    if !available {
        tracing::debug!(
            "Wayland socket 不存在 ({}), 跳过 Wayland 后端",
            socket.display()
        );
    }
    available
}

/// 构造携带 `runner::UserEvent` 的事件循环构建器（跨平台共用）。
///
/// # 为什么单独抽成一个「不带 cfg 门控」的函数
///
/// winit 0.30 里 `with_user_event()` 是 `EventLoop<T>` 的关联函数（返回
/// `EventLoopBuilder<T>`），**不是** `EventLoopBuilder<T>` 的——后者的 `impl`
/// 块里只有 `build()`。而 X11/Wayland 分支整体挂在 `#[cfg(target_os = "linux")]`
/// 下：在 Windows 上开发时这段代码被 cfg 剥离、永不参与类型检查，写错了只能等
/// ubuntu CI 炸（2026-10-16 a047e613e 引入，潜伏 2 天）。
///
/// 收敛到本函数后，该 API 调用在本机任意平台的 `cargo check` 下都会被检查，
/// 同类误用不再需要跨平台 CI 才能暴露。
fn user_event_builder() -> EventLoopBuilder<runner::UserEvent> {
    EventLoop::<runner::UserEvent>::with_user_event()
}

/// Android：事件循环必须用 `with_android_app` 绑定 Activity，否则 winit 直接 panic。
#[cfg(target_os = "android")]
fn create_event_loop(
    app: Option<PlatformApp>,
) -> Result<EventLoop<runner::UserEvent>, winit::error::EventLoopError> {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    let mut builder = user_event_builder();
    if let Some(app) = app {
        builder.with_android_app(app);
    }
    builder.build()
}

/// 根据当前显示服务器环境自动创建事件循环。
///
/// # 设计原理
///
/// winit 0.30 内部有一个进程级原子标志 `EVENT_LOOP_CREATED`，整个进程
/// 生命周期内只能调用一次 `EventLoopBuilder::build()`。因此无法通过
/// "try Wayland → 失败 → retry X11" 的方式实现回退——第一次 `build()`
/// 即使失败也会永久锁定该标志。
///
/// 解决方案：在调用 `build()` **之前** 通过检查 Wayland socket 实际存在性
/// 来决策选用哪个后端。
#[cfg(target_os = "linux")]
fn create_event_loop(
    _app: Option<PlatformApp>,
) -> Result<EventLoop<runner::UserEvent>, winit::error::EventLoopError> {
    use winit::platform::x11::EventLoopBuilderExtX11;

    let has_x11 = std::env::var("DISPLAY").is_ok();
    let wayland_ok = wayland_compositor_available();

    if wayland_ok {
        // Wayland 合成器真正在运行，让 winit 默认走 Wayland
        tracing::info!("Wayland 合成器检测可用，使用 Wayland 后端");
        user_event_builder().build()
    } else if has_x11 {
        // Wayland 不可用，强制 X11 后端
        tracing::info!("使用 X11 后端");
        let mut builder = user_event_builder();
        builder.with_x11();
        builder.build()
    } else {
        // 两个显示服务器都不可用，尝试默认（会失败但给出明确错误）
        tracing::error!("未检测到显示服务器（Wayland socket 和 DISPLAY 均不可用）");
        user_event_builder().build()
    }
}

/// 非 Linux / 非 Android 平台直接使用默认事件循环创建（REND-019 PR-C：事件循环参数化
/// `UserEvent`——device-lost 通知链路 §1.3 C2 的前置改造）。
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn create_event_loop(
    _app: Option<PlatformApp>,
) -> Result<EventLoop<runner::UserEvent>, winit::error::EventLoopError> {
    user_event_builder().build()
}

/// 启动 Lumino（桌面与 Android 共用）。
///
/// `app` 只有 Android 会传 `Some(AndroidApp)`；桌面传 `None`。
pub fn run(app: Option<PlatformApp>) -> anyhow::Result<()> {
    // GPU 检测子进程模式：必须在日志 / puffin / 内存监控 / clap **之前**分支。
    // 子进程只做一次无头检测，不需要事件循环、音频与 UI；晚一步就可能抢 8585
    // 端口、起看门狗线程，或直接被 clap 以「未知参数」拒绝（退出码 2）。
    //
    // 合并自 dev（`feat(gpu): 全平台 GPU 指纹探测改子进程隔离`）：本函数由
    // 原 `main.rs` 搬迁而来，分支必须随之搬迁，否则该特性在本分支静默失效。
    if device_probe::is_child_mode() {
        device_probe::run_child_mode();
    }

    logging::init();

    // 启动 puffin 性能分析服务器 - 保持存活直到程序结束（Android 上为 no-op）
    let _puffin_holder = PuffinServerHolder::new();

    // 启动内存监控：主监控（95% → abort）+ 看门狗（100% → SIGKILL）
    // 看门狗完全独立，用 /proc/{pid} 而非 /proc/self，系统可用 < 350MB 也触发
    if !lumino_diagnostics::memory_monitor::spawn_all_monitors() {
        tracing::warn!("内存监控线程启动失败，程序继续运行但缺少 OOM 防护");
    }

    // 事件循环必须在「拥有 Looper 的线程」上跑（Android 的 android_main 线程），
    // 而 tokio 运行时只需在当前线程**进入上下文**——`resumed`/`about_to_wait` 里的
    // `tokio::spawn` 依赖它。桌面语义与原先 `#[tokio::main]` 等价。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| anyhow::anyhow!("创建 tokio 运行时失败: {error}"))?;
    let _runtime_guard = runtime.enter();

    let cli = cli::Cli::parse_args();
    let test_config = cli.get_test_config();

    let event_loop = create_event_loop(app)?;

    // REND-019 PR-C §1.3（C1/C2）：device-lost 回调 → EventLoopProxy（无锁唤醒；
    // 回调线程只做原子置位 + send_event，重连决策由 UI 主循环消费事件后发起）
    let lost_proxy = event_loop.create_proxy();
    lumino_gfx::set_device_lost_notifier(move |reason, message| {
        let _ = lost_proxy.send_event(runner::UserEvent::GpuDeviceLost {
            reason: format!("{reason:?}"),
            message,
        });
    });

    let proxy = event_loop.create_proxy();
    lumino_ui::event::set_waker(move || {
        let _ = proxy.send_event(runner::UserEvent::UiWake);
    });

    // REND-019 PR-D §1.5 L1：LGS（GPU 合成）**独立 device** 丢失 → UserEvent
    // （独立链路；LGS 与编辑器零共享 device——红线 8）
    let lgs_proxy = event_loop.create_proxy();
    lumino_midi_io::set_lgs_device_lost_hook(move |reason, message| {
        let _ = lgs_proxy.send_event(runner::UserEvent::LgsDeviceLost { reason, message });
    });

    let mut runner = runner::Runner::default();

    // 如果是测试模式，设置测试配置
    if let Some(config) = test_config {
        runner.set_test_config(config);
    }

    // 设置日志功能
    if cli.log_memory_usage() {
        runner.set_log_memory_usage(true);
        tracing::info!("已启用 memory-usage 日志（每2000ms输出各组件内存占用）");
    }

    // 主线程即 UI 线程：将整个 GUI 生命周期的分配归因到 Ui。
    // with_tag 是线程局部的，run_app 在调用线程上运行，
    // 因此该线程的所有分配（编辑器状态、空间索引、iced 每帧 widget 树等）均归 Ui。
    // 其中 MIDI 解析路径在 document.rs 内层已用 Midi 标签覆盖，音频渲染线程在 synth.rs 内单独标 Audio。
    let loop_result =
        lumino_diagnostics::memtrace::with_tag(lumino_diagnostics::memtrace::AllocTag::Ui, || {
            event_loop.run_app(&mut runner)
        });
    log::info!("事件循环已退出: {loop_result:?}");
    loop_result?;

    Ok(())
}

/// Android 入口：符号名固定为 `android_main`，由 android-activity 胶水调用。
///
/// edition 2024 下 `#[no_mangle]` 必须写成 `#[unsafe(no_mangle)]`。
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: PlatformApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Trace)
            .with_tag("lumino"),
    );

    // android-activity 在 Activity 被系统销毁并重建时，会**再次**调用 `android_main`。
    // 而 winit 的 `EventLoop` 每进程只能创建一个（`EventLoopError::RecreationAttempt`），
    // 第二次调用一旦 `return`，胶水就会 `finishAndRemoveTask()` —— 用户看到的现象是
    // 「切后台再回来，应用自己消失」。这里让第二次调用**不返回**（挂起该线程），
    // 保留第一次调用创建的、仍然持有窗口与 GPU 资源的事件循环。
    static FIRST_INVOCATION: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(true);
    if !FIRST_INVOCATION.swap(false, std::sync::atomic::Ordering::SeqCst) {
        log::warn!("android_main 被重复调用（Activity 重建）：挂起该线程，保留既有事件循环");
        loop {
            std::thread::park();
        }
    }

    // 必须最早注入：Android 上没有 $HOME/XDG，Storage::new() 依赖这个目录，
    // 缺了就在启动第一步 Err(NotFound) → 优雅退出（看上去像"闪退"）。
    match app.internal_data_path() {
        Some(dir) => {
            tracing::info!("Android 数据目录: {}", dir.display());
            storage::set_android_data_dir(dir);
        }
        None => tracing::error!("Android 无法获取 internal_data_path，配置将无法持久化"),
    }

    let outcome = run(Some(app));
    match &outcome {
        Ok(()) => log::info!("主循环正常退出（Activity 将被 finish）"),
        Err(error) => log::error!("Android 主循环异常退出: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期契约：`with_user_event()` 属于 `EventLoop<T>`，返回 `EventLoopBuilder<T>`。
    ///
    /// 若有人改回 `EventLoopBuilder::<T>::with_user_event()`，本测试在**所有平台**
    /// 编译失败，而不是只在 ubuntu CI 红——这是对 cfg 剥离盲区的直接封堵。
    /// 构造 builder 只写入平台属性结构体（各平台 `Default` 均无副作用），
    /// 不触碰 winit 的 `EVENT_LOOP_CREATED` 一次性标志，可在测试线程安全调用。
    #[test]
    fn user_event_builder_matches_winit_api() {
        fn assert_builder_type(_: EventLoopBuilder<runner::UserEvent>) {}

        assert_builder_type(user_event_builder());
    }
}
