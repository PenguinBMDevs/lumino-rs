// Windows 子系统控制（由 build.rs 根据 DEBUG env var 自动发射 cfg 标记）：
//   release profile（DEBUG=false, windows_gui_subsystem 标记激活）→ windows 子系统，隐藏终端
//   debug / fast-release（DEBUG=true, 无标记）→ console 子系统，显示终端
#![cfg_attr(
    all(target_os = "windows", windows_gui_subsystem),
    windows_subsystem = "windows"
)]

use winit::event_loop::EventLoop;

/// 全局内存追踪分配器，按子系统统计堆分配。
#[global_allocator]
static GLOBAL_ALLOC: lumino_memtrace::TaggedAlloc = lumino_memtrace::TaggedAlloc;

mod cli;
mod constants;
mod logging;
mod platform;
mod runner;
mod services;
mod storage;

/// Puffin 服务器包装器，确保在程序结束时才释放
struct PuffinServerHolder(#[allow(dead_code)] Option<puffin_http::Server>);

impl PuffinServerHolder {
    fn new() -> Self {
        puffin::set_scopes_on(true);
        let server = puffin_http::Server::new("0.0.0.0:8585").ok();
        if server.is_some() {
            tracing::info!("Puffin profiler server started on http://127.0.0.1:8585");
            tracing::info!("Run `cargo install puffin_viewer && puffin_viewer` to view flamegraph");
        } else {
            tracing::warn!("Failed to start Puffin profiler server");
        }
        Self(server)
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
fn create_event_loop() -> Result<EventLoop<()>, winit::error::EventLoopError> {
    use winit::platform::x11::EventLoopBuilderExtX11;

    let has_x11 = std::env::var("DISPLAY").is_ok();
    let wayland_ok = wayland_compositor_available();

    if wayland_ok {
        // Wayland 合成器真正在运行，让 winit 默认走 Wayland
        tracing::info!("Wayland 合成器检测可用，使用 Wayland 后端");
        EventLoop::builder().build()
    } else if has_x11 {
        // Wayland 不可用，强制 X11 后端
        tracing::info!("使用 X11 后端");
        let mut builder = EventLoop::builder();
        builder.with_x11();
        builder.build()
    } else {
        // 两个显示服务器都不可用，尝试默认（会失败但给出明确错误）
        tracing::error!("未检测到显示服务器（Wayland socket 和 DISPLAY 均不可用）");
        EventLoop::builder().build()
    }
}

/// 非 Linux 平台直接使用默认事件循环创建。
#[cfg(not(target_os = "linux"))]
fn create_event_loop() -> Result<EventLoop<()>, winit::error::EventLoopError> {
    EventLoop::new()
}

#[tokio::main]
async fn main() -> Result<(), winit::error::EventLoopError> {
    logging::init();

    // 启动 puffin 性能分析服务器 - 保持存活直到程序结束
    let _puffin_holder = PuffinServerHolder::new();

    // 启动内存监控：主监控（95% → abort）+ 看门狗（100% → SIGKILL）
    // 看门狗完全独立，用 /proc/{pid} 而非 /proc/self，系统可用 < 350MB 也触发
    if !lumino_memory_monitor::spawn_all_monitors() {
        tracing::warn!("内存监控线程启动失败，程序继续运行但缺少 OOM 防护");
    }

    let cli = cli::Cli::parse_args();
    let test_config = cli.get_test_config();

    let event_loop = create_event_loop()?;
    let proxy = event_loop.create_proxy();
    lumino_ui::event::set_waker(move || {
        let _ = proxy.send_event(());
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

    event_loop.run_app(&mut runner)
}
