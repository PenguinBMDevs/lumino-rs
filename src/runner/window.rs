use winit::event::WindowEvent;

use super::RunnerInner;

impl RunnerInner {
    pub(super) fn handle_main_window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                self.handle_main_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_moved(position);
            }
            WindowEvent::Touch(touch) => {
                self.ui.cursor_moved(touch.location);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_main_resize(size);
            }
            WindowEvent::Moved(pos) => {
                self.storage.ui_state.patch(|state| {
                    state.x = Some(pos.x);
                    state.y = Some(pos.y);
                });
            }
            WindowEvent::CloseRequested => {
                self.window.request_redraw();
            }
            _ => (),
        }

        self.ui.handle_events(event, self.modifiers);
    }

    fn handle_main_redraw(&mut self) {
        if self.resized {
            let size = self.window.inner_size();
            self.ui.resize(size.width, size.height);
            self.gfx.resize(size.width, size.height);
            self.resized = false;
        }

        if self
            .gfx
            .with_frame(|frame, view| self.ui.redraw_requested(frame, view, &self.gfx))
            .is_err()
        {
            self.window.request_redraw();
        };
    }

    fn handle_main_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.storage.ui_state.patch(|state| {
            state.w = size.width;
            state.h = size.height;
            state.is_maximized = self.window.is_maximized();
        });
        self.resized = true;
    }

    pub(super) fn handle_window_actions(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if let Some(action) = self.ui.take_window_action() {
            use lumino_ui::window::TrafficAction;
            match action {
                TrafficAction::Minimize => {
                    self.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    let is_maximized = self.window.is_maximized();
                    self.window.set_maximized(!is_maximized);
                }
                TrafficAction::Close => {
                    event_loop.exit();
                }
            }
        }

        if self.ui.take_drag()
            && let Err(e) = self.window.drag_window()
        {
            tracing::warn!("拖动窗口失败: {}", e);
        }
    }

    pub(super) fn save_storage(&mut self) {
        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}
