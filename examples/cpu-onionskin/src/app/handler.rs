//! `ApplicationHandler` 实现：窗口事件转发与等待策略。

use std::time::{Duration, Instant};

use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

use super::App;
use crate::camera::Camera;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init_window(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.needs_redraw = false;
                self.redraw(event_loop);
            }
            WindowEvent::Resized(size) => self.handle_resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    self.handle_resize(size);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x, position.y);
                if self.editor.drag.is_some() {
                    self.update_drag_from_mouse();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => self.handle_mouse(state, button),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::MouseWheel { delta, .. } => self.handle_wheel(delta),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.handle_key(event_loop, &event.logical_key);
                }
            }
            WindowEvent::Occluded(false) => {
                self.needs_redraw = true;
                self.next_frame = Instant::now();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(deadline) = self.smoke_deadline
            && Instant::now() >= deadline
        {
            if !self.idle_reported {
                self.stats.print_idle(self.index.total_notes);
                self.idle_reported = true;
            }
            event_loop.exit();
            return;
        }
        let bench_active = self.bench.as_ref().is_some_and(|bench| !bench.finished);
        if bench_active {
            // 基准模式不等 15.6ms 系统时钟：present 本身是节流点，工作耗时才是验收数据。
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        let moving = self.camera.as_ref().is_some_and(Camera::is_moving) || self.needs_redraw;
        if moving {
            let now = Instant::now();
            if now >= self.next_frame
                && let Some(window) = &self.window
            {
                window.request_redraw();
            }
            let next = self.next_frame.max(now + Duration::from_millis(1));
            let next = self
                .smoke_deadline
                .map_or(next, |deadline| next.min(deadline));
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        } else if let Some(deadline) = self.smoke_deadline {
            // 静止验证：不 request_redraw，只等冒烟截止时间退出。
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
