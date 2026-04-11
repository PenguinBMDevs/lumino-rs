// 防止 Windows release 模式显示控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use winit::event_loop::EventLoop;

mod cli;
mod logging;
mod platform;
mod runner;
mod services;
mod storage;

/// Puffin 服务器包装器，确保在程序结束时才释放
struct PuffinServerHolder(Option<puffin_http::Server>);

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

#[tokio::main]
async fn main() -> Result<(), winit::error::EventLoopError> {
    logging::init();

    // 启动 puffin 性能分析服务器 - 保持存活直到程序结束
    let _puffin_holder = PuffinServerHolder::new();

    let cli = cli::Cli::parse_args();
    let test_config = cli.get_test_config();

    let event_loop = EventLoop::new()?;
    let proxy = event_loop.create_proxy();
    lumino_core::event::set_waker(move || {
        let _ = proxy.send_event(());
    });

    let mut runner = runner::Runner::default();

    // 如果是测试模式，设置测试配置
    if let Some(config) = test_config {
        runner.set_test_config(config);
    }

    event_loop.run_app(&mut runner)
}
