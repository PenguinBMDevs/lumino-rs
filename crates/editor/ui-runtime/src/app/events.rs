//! 原生运行时事件处理：winit → egui 输入与编辑器动作。
//!
//! `window_event` 只做事件分类，具体处理按事件类型拆成 `on_*`；输入归属
//! （egui 浮层 vs 编辑器）仍由 `egui_owns_pointer` 统一判定。

use std::sync::Arc;
use std::time::Instant;

use egui::{Event as EguiEvent, PointerButton as EguiButton, Pos2, Vec2};
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use super::state::{GfxCanvas, Gpu, NativeApp};

impl crate::RuntimeApp for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Lumino · egui native runtime")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!("创建窗口失败: {error}");
                event_loop.exit();
                return;
            }
        };
        match Gpu::new(window.clone()) {
            Ok(gpu) => match GfxCanvas::new(&gpu.device, &gpu.queue, gpu.config.format) {
                Ok(canvas) => {
                    self.canvas = Some(canvas);
                    self.gpu = Some(gpu);
                }
                Err(error) => {
                    tracing::error!("初始化 gfx 画布失败: {error}");
                    event_loop.exit();
                    return;
                }
            },
            Err(error) => {
                tracing::error!("初始化 wgpu 失败: {error}");
                event_loop.exit();
                return;
            }
        }
        self.ctx.set_visuals(egui::Visuals::dark());
        self.ctx.set_pixels_per_point(window.scale_factor() as f32);
        let mut root = lumino_ui::Root::new(&lumino_core::storage::config::UiConfig::default());
        // 空工程 → 建两条音轨（0 指挥 / 1 普通），当前轨设为 1，便于铅笔落音符。
        let mut doc = lumino_midi_loader::MidiDocument::empty_with_tracks(2, 480);
        doc.tempo_changes = vec![(0, 120.0)];
        doc.time_signatures = vec![(0, 4, 4)];
        root.set_midi_document(doc);
        root.editor.editor_state.view.ppq = 480;
        root.editor.editor_state.data.current_track = 1;
        // 走生产工具栏事件：同时更新 toolbar 与 editor 的工具状态。
        root.update(lumino_ui_core::toolbar_event::Event::tool_selected(
            lumino_core::Tool::Pencil,
        ));
        self.root = Some(root);
        self.sync_editor_canvas(&window);
        self.start = Some(Instant::now());
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let scale = window.scale_factor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.on_resized(size, &window),
            WindowEvent::ScaleFactorChanged { .. } => self.on_scale_factor_changed(&window),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(position, scale);
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                self.events.push(EguiEvent::PointerGone);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse_input(state, button);
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta, scale),
            WindowEvent::KeyboardInput { event, .. } => self.on_keyboard_input(event),
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // 探针保持 60Hz 重绘（egui 悬停/按钮反馈需要连续帧）。
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(16),
        ));
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl NativeApp {
    fn on_resized(&mut self, size: PhysicalSize<u32>, window: &Window) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.resize(size.width, size.height);
        }
        self.sync_editor_canvas(window);
        window.request_redraw();
    }

    fn on_scale_factor_changed(&mut self, window: &Window) {
        self.ctx.set_pixels_per_point(window.scale_factor() as f32);
        self.sync_editor_canvas(window);
        window.request_redraw();
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>, scale: f64) {
        let logical = position.to_logical::<f32>(scale);
        let pos = Pos2::new(logical.x, logical.y);
        self.pointer = Some(pos);
        self.events.push(EguiEvent::PointerMoved(pos));
        // 指针在 egui 浮层上时不再拖动画布；已开始的拖拽必须继续
        //（松手可能落在浮层上）。
        if self.editor_dragging
            && let Some(local) = self.editor_local(pos)
        {
            self.send_editor(lumino_message::EditorAction::Moved(local));
        }
    }

    fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        let Some(pos) = self.pointer else {
            return;
        };
        self.events.push(EguiEvent::PointerButton {
            pos,
            button: pointer_button(button),
            pressed: state == ElementState::Pressed,
            modifiers: modifiers(self.modifiers),
        });
        // 输入归属：egui 浮层占住的位置不得再落到编辑器。
        // `layer_id_at` 覆盖稳定态，`blocked` 兜住 `Area` 首帧
        // sizing pass（图层命中不到）的空窗。
        let owns = self.egui_owns_pointer(pos);
        if button == MouseButton::Left {
            if state == ElementState::Pressed {
                if !owns
                    && !self.editor_dragging
                    && let Some(local) = self.editor_local(pos)
                {
                    self.send_editor(lumino_message::EditorAction::Pressed {
                        pos: local,
                        shift: self.modifiers.shift_key(),
                        ctrl: self.modifiers.control_key(),
                    });
                    self.editor_dragging = true;
                }
            } else if self.editor_dragging {
                // 抬起一律收尾：拖拽可能结束在浮层上。
                self.send_editor(lumino_message::EditorAction::Released);
                self.editor_dragging = false;
            }
        }
    }

    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta, scale: f64) {
        let (unit, delta) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (egui::MouseWheelUnit::Line, Vec2::new(x, y)),
            MouseScrollDelta::PixelDelta(delta) => {
                let logical = delta.to_logical::<f32>(scale);
                (egui::MouseWheelUnit::Point, Vec2::new(logical.x, logical.y))
            }
        };
        self.events.push(EguiEvent::MouseWheel {
            unit,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: modifiers(self.modifiers),
        });
    }

    fn on_keyboard_input(&mut self, event: KeyEvent) {
        if event.state == ElementState::Pressed
            && let Some(key) = egui_key(&event.logical_key)
        {
            self.events.push(EguiEvent::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: modifiers(self.modifiers),
            });
        }
        if let Some(text) = &event.text
            && !text.is_empty()
            && !self.modifiers.control_key()
            && !self.modifiers.super_key()
        {
            self.events.push(EguiEvent::Text(text.to_string()));
        }
    }

    /// 把编辑器画布尺寸同步为「内容区」逻辑尺寸（上一代 布局原本由 widget 写入）。
    fn sync_editor_canvas(&mut self, window: &Window) {
        let Some(root) = self.root.as_mut() else {
            return;
        };
        let ppp = window.scale_factor() as f32;
        let size = window.inner_size();
        let width = (size.width as f32 / ppp - lumino_ui_egui::layout::SIDEBAR_W).max(1.0);
        let height =
            (size.height as f32 / ppp - lumino_ui_egui::layout::TITLEBAR_H - root.toolbar.height)
                .max(1.0);
        root.editor.editor_state.canvas.set_size(width, height);
        // 画布在窗口中的真实原点（左栏右缘 / 标题栏 + 工具栏下缘）：
        // 渲染线程的 `canvas_offset` 与 egui 的输入换算必须同源，否则音符
        // 会画在错误位置（或落到 scissor 之外）。
        root.editor.editor_state.canvas.set_offset(
            lumino_ui_egui::layout::SIDEBAR_W,
            lumino_ui_egui::layout::TITLEBAR_H + root.toolbar.height,
        );
    }

    /// 窗口逻辑坐标 → 编辑器画布局部坐标（画布左上 = 左栏右缘 + 标题栏/工具栏下缘）。
    fn editor_local(&self, pos: Pos2) -> Option<lumino_message::Point2> {
        let left = lumino_ui_egui::layout::SIDEBAR_W;
        let toolbar_h = self
            .root
            .as_ref()
            .map(|root| root.toolbar.height)
            .unwrap_or(72.0);
        let top = lumino_ui_egui::layout::TITLEBAR_H + toolbar_h;
        if pos.x < left || pos.y < top {
            return None;
        }
        Some(lumino_message::Point2 {
            x: pos.x - left,
            y: pos.y - top,
        })
    }

    fn send_editor(&mut self, action: lumino_message::EditorAction) {
        // 指针移动期间每次 `Moved` 都会走这里：只能打 trace，避免 `RUST_LOG=debug`
        // 时把正常日志冲掉。
        tracing::trace!(?action, "native editor action");
        if let Some(root) = self.root.as_mut() {
            root.update(lumino_ui_core::Message::EditorAction(action));
            tracing::trace!(
                "native editor state: tool={:?} track={} count={} doc={}",
                root.editor.editor_state.tool,
                root.editor.editor_state.data.current_track,
                root.editor.editor_state.data.current_track_note_count(),
                root.editor.editor_state.data.document.is_some()
            );
        }
    }

    /// 指针是否被 egui 占住（浮层图层 ∪ 上一帧占用表）。
    fn egui_owns_pointer(&self, pos: Pos2) -> bool {
        let layer = self
            .ctx
            .layer_id_at(pos)
            .is_some_and(|id| id.order != egui::Order::Background);
        layer || self.blocked.iter().any(|rect| rect.contains(pos))
    }
}

fn pointer_button(button: MouseButton) -> EguiButton {
    match button {
        MouseButton::Left => EguiButton::Primary,
        MouseButton::Right => EguiButton::Secondary,
        MouseButton::Middle => EguiButton::Middle,
        _ => EguiButton::Extra1,
    }
}

fn modifiers(state: ModifiersState) -> egui::Modifiers {
    egui::Modifiers {
        alt: state.alt_key(),
        ctrl: state.control_key(),
        shift: state.shift_key(),
        mac_cmd: state.super_key(),
        command: state.control_key() || state.super_key(),
    }
}

fn egui_key(key: &Key) -> Option<egui::Key> {
    Some(match key {
        Key::Named(NamedKey::Escape) => egui::Key::Escape,
        Key::Named(NamedKey::Enter) => egui::Key::Enter,
        Key::Named(NamedKey::Space) => egui::Key::Space,
        Key::Named(NamedKey::Tab) => egui::Key::Tab,
        Key::Named(NamedKey::Backspace) => egui::Key::Backspace,
        Key::Named(NamedKey::Delete) => egui::Key::Delete,
        Key::Named(NamedKey::ArrowLeft) => egui::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => egui::Key::ArrowRight,
        Key::Named(NamedKey::ArrowUp) => egui::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => egui::Key::ArrowDown,
        _ => return None,
    })
}
