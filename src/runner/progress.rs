use std::sync::Arc;

use winit::{dpi, event::WindowEvent, window::WindowAttributes};

use super::RunnerInner;

pub const PROGRESS_WINDOW_WIDTH: u32 = 500;
pub const PROGRESS_WINDOW_HEIGHT: u32 = 200;

impl RunnerInner {
    pub(super) fn is_progress_window(&self, window_id: winit::window::WindowId) -> bool {
        self.progress_window
            .as_ref()
            .is_some_and(|w| w.id() == window_id)
    }

    pub(super) fn handle_progress_window_event(&mut self, event: WindowEvent) {
        let Some(progress_window) = self.progress_window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                self.handle_progress_redraw(&progress_window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref mut progress_ui) = self.progress_ui {
                    progress_ui.cursor_moved(position);
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.progress_modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_progress_resize(size, &progress_window);
            }
            WindowEvent::CloseRequested => {
                self.close_progress_window();
            }
            _ => {
                if let Some(ref mut progress_ui) = self.progress_ui {
                    progress_ui.handle_events(event, self.progress_modifiers);
                }
            }
        }
    }

    fn handle_progress_redraw(&mut self, progress_window: &Arc<winit::window::Window>) {
        if let Some(ref mut progress_ui) = self.progress_ui
            && let Some(ref progress_gfx) = self.progress_gfx
            && progress_gfx
                .with_frame(|a, b| progress_ui.redraw_requested(a, b))
                .is_err()
        {
            progress_window.request_redraw();
        }
    }

    fn handle_progress_resize(
        &mut self,
        size: winit::dpi::PhysicalSize<u32>,
        progress_window: &Arc<winit::window::Window>,
    ) {
        if let Some(ref mut progress_ui) = self.progress_ui {
            progress_ui.resize(size.width, size.height);
        }
        if let Some(ref mut progress_gfx) = self.progress_gfx {
            progress_gfx.resize(size.width, size.height);
        }
        progress_window.request_redraw();
    }

    pub(super) fn close_progress_window(&mut self) {
        self.progress = None;
        self.progress_window = None;
        self.progress_gfx = None;
        self.progress_ui = None;
    }

    pub(super) fn update_progress_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if self.progress.is_some() && self.progress_window.is_none() {
            self.create_progress_window(event_loop);
        } else if self.progress.is_none() && self.progress_window.is_some() {
            self.close_progress_window();
        }
    }

    fn create_progress_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_inner_size(dpi::LogicalSize {
                width: PROGRESS_WINDOW_WIDTH,
                height: PROGRESS_WINDOW_HEIGHT,
            })
            .with_title("MIDI 处理进度")
            .with_decorations(true)
            .with_visible(true);

        let progress_window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("创建进度窗口失败"),
        );

        let physical_size = progress_window.inner_size();

        let progress_gfx = futures::executor::block_on(lumino_gfx::Context::new(
            progress_window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .expect("初始化进度窗口图形上下文失败");

        let progress_ui = lumino_ui::Host::new(
            progress_window.clone(),
            physical_size.width,
            physical_size.height,
            &self.storage.config.get().ui,
            &progress_gfx,
            true,
        );

        self.progress_window = Some(progress_window);
        self.progress_gfx = Some(progress_gfx);
        self.progress_ui = Some(progress_ui);
    }

    pub(super) fn process_progress_messages(&mut self) {
        while let Ok((msg, progress)) = self.progress_rx.try_recv() {
            self.handle_progress_message(msg, progress);
        }
    }

    fn handle_progress_message(&mut self, msg: String, progress: f64) {
        // 进度完成（>= 1.0）时，关闭进度窗口
        if progress >= 1.0 {
            // 先显示完成消息，然后关闭窗口
            self.ui.update_progress(Some((msg.clone(), progress)));
            if let Some(ref mut progress_ui) = self.progress_ui {
                progress_ui.update_progress(Some((msg, progress)));
            }
            // 请求重绘以显示最终状态
            self.window.request_redraw();
            if let Some(ref progress_window) = self.progress_window {
                progress_window.request_redraw();
            }
            // 关闭进度窗口
            self.progress = None;
            return;
        }

        self.progress = Some((msg.clone(), progress));
        self.ui.update_progress(Some((msg.clone(), progress)));
        if let Some(ref mut progress_ui) = self.progress_ui {
            progress_ui.update_progress(Some((msg, progress)));
        }
        self.window.request_redraw();
        if let Some(ref progress_window) = self.progress_window {
            progress_window.request_redraw();
        }
    }
}
