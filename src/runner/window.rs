use winit::event::WindowEvent;

use super::RunnerInner;
use lumino_core::storage::config::{SynthBackend, UiConfig};

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
        // 获取当前 UI 中的设置
        let new_preferred_backend = self.ui.settings().synth_backend;
        let new_soundfont_path = self.ui.settings().soundfont_path.clone();

        // 获取当前存储的配置
        let old_config = self.storage.config.get();
        let old_preferred_backend = old_config.ui.preferred_backend;
        let old_soundfont_path = &old_config.ui.soundfont_path;

        // 检查合成器相关设置是否改变
        let backend_changed = new_preferred_backend != old_preferred_backend;
        let soundfont_changed = new_soundfont_path != *old_soundfont_path;

        if backend_changed || soundfont_changed {
            tracing::info!(
                "合成器设置已改变: backend {} -> {}, soundfont {} -> {}",
                old_preferred_backend,
                new_preferred_backend,
                if old_soundfont_path.is_empty() {
                    "(空)"
                } else {
                    old_soundfont_path
                },
                if new_soundfont_path.is_empty() {
                    "(空)"
                } else {
                    &new_soundfont_path
                }
            );
            // 标记需要重新初始化 MIDI
            self.midi_needs_reinit = true;
        }

        // 保存配置
        self.storage.config.patch(|config| {
            config.ui.preferred_backend = new_preferred_backend;
            config.ui.soundfont_path = new_soundfont_path;
        });

        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }

    /// 如果设置改变，重新初始化 MIDI 输出
    pub(super) fn reinit_midi_if_needed(&mut self) {
        if !self.midi_needs_reinit {
            return;
        }

        self.midi_needs_reinit = false;

        // 获取当前配置
        let ui_config = self.storage.config.get().ui.clone();

        tracing::info!(
            "重新初始化 MIDI 输出，使用偏好后端: {:?}",
            ui_config.preferred_backend
        );

        // 关闭旧的 MIDI 输出
        if let Some(old_output) = self.midi_output.take() {
            drop(old_output);
        }
        // 注意：midi_api 会在新 API 创建时被替换

        // 重新初始化
        let (new_api, new_output, new_backend) = super::Runner::init_midi_output(&ui_config);
        self.midi_api = new_api;
        self.midi_output = new_output;
        self.active_synth_backend = new_backend.unwrap_or(SynthBackend::System);

        tracing::info!(
            "MIDI 输出已重新初始化，实际后端: {:?}",
            self.active_synth_backend
        );
    }
}
