// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use winit::event_loop::EventLoop;

mod logging;
mod platform;
mod runner;

fn main() -> Result<(), winit::error::EventLoopError> {
    logging::init();

    // Initialize winit
    let event_loop = EventLoop::new()?;

    let mut runner = runner::Runner::default();
    event_loop.run_app(&mut runner)
}
