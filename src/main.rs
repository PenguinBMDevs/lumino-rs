// 防止 Windows release 模式显示控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use winit::event_loop::EventLoop;

mod cli;
mod logging;
mod platform;
mod runner;
mod services;
mod storage;

#[tokio::main]
async fn main() -> Result<(), winit::error::EventLoopError> {
    logging::init();

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
