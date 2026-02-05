// 防止Windows release模式显示控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use winit::event_loop::EventLoop;

mod logging;
mod platform;
mod runner;
mod storage;

#[tokio::main]
async fn main() -> Result<(), winit::error::EventLoopError> {
    logging::init();

    let event_loop = EventLoop::new()?;
    let mut runner = runner::Runner::default();
    event_loop.run_app(&mut runner)
}
