//! 画布输入直连（winit → 编辑器），不再经过 iced `Grid` program。
//!
//! `LUMINO_EGUI_CANVAS_INPUT=1` 时启用：宿主把鼠标按下/移动/释放、滚轮、
//! 修饰键与失焦直接翻译成 [`CanvasEvent`]，与 iced 路径共用
//! `ui_editor::grid::canvas_input::dispatch` 的分派语义。
//!
//! 启用后 `process_pending_events` 不再构建 iced widget 树（主窗口），
//! 避免同一击被处理两次；对话框窗口没有画布，仍走原路径。

use lumino_ui_editor::grid::canvas_input::{CanvasButton, CanvasEvent};

use crate::host::Host;

/// 画布输入是否由宿主直接驱动。
///
/// **默认开启**：钢琴卷帘画布（音符编辑 / 框选 / 滚轮 / 右键菜单）与滚动条
/// 均已由 egui + winit 直连接管。`LUMINO_EGUI_CANVAS_INPUT=0` 回退到 iced
/// `Grid` 通道，用于对照排查。
pub(crate) fn canvas_input_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_CANVAS_INPUT")
            .map(|value| value != "0")
            .unwrap_or(true)
    })
}

impl Host {
    /// 主编辑器窗口：画布输入由本模块接管（对话框 / 进度窗没有画布）。
    ///
    /// 纵向卷帘与横向共用同一套分派（`canvas_input::vertical`）；走带、瀑布流
    /// 与导出面板没有钢琴卷帘画布，仍走 iced 通道。
    pub(crate) fn canvas_input_active(&self) -> bool {
        canvas_input_enabled()
            && !self.root.state.is_dialog_window
            && !self.root.is_progress_window
            && !self.root.is_device_warning_window
            && !self.root.is_arrangement_mode()
            && self.root.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
            && self.root.sidebar.piano_roll_visible
            && self.render_ctx.note_renderer.is_some()
    }

    /// 窗口逻辑坐标 → 画布局部坐标（无画布时返回 `None`）。
    fn canvas_local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let canvas = &self.root.editor.editor_state.canvas;
        if canvas.size_x <= 0.0 || canvas.size_y <= 0.0 {
            return None;
        }
        Some((x - canvas.offset_x, y - canvas.offset_y))
    }

    /// 当前光标（逻辑坐标）在画布内的局部坐标。
    fn pointer_local(&self) -> Option<(f32, f32)> {
        let position = self.window_ctx.cursor_position?;
        self.canvas_local(position.x, position.y)
    }

    /// 分派一条画布事件；产生消息时立即处理。
    pub(crate) fn dispatch_canvas(&mut self, event: CanvasEvent) {
        if !self.canvas_input_active() {
            return;
        }
        let message = if self.root.sidebar.is_vertical_roll() {
            lumino_ui_editor::grid::canvas_input::vertical::dispatch(
                &self.root.editor,
                &mut self.canvas_input,
                event,
            )
        } else {
            lumino_ui_editor::grid::canvas_input::dispatch(
                &self.root.editor,
                &mut self.canvas_input,
                event,
            )
        };
        if let Some(message) = message {
            self.process_message(message);
            self.window_ctx.window.request_redraw();
        }
        self.refresh_canvas_cursor();
    }

    /// 指针按下 / 释放（按钮级）。
    pub(crate) fn dispatch_canvas_button(
        &mut self,
        pressed: bool,
        button: winit::event::MouseButton,
    ) {
        if !self.canvas_input_active() {
            return;
        }
        let Some(local) = self.pointer_local() else {
            return;
        };
        let button = match button {
            winit::event::MouseButton::Left => CanvasButton::Left,
            winit::event::MouseButton::Right => CanvasButton::Right,
            _ => return,
        };
        let event = if pressed {
            CanvasEvent::Press { local, button }
        } else if button == CanvasButton::Left {
            CanvasEvent::Release
        } else {
            return;
        };
        self.dispatch_canvas(event);
    }

    /// 指针移动。
    pub(crate) fn dispatch_canvas_moved(&mut self) {
        if !self.canvas_input_active() {
            return;
        }
        let Some(local) = self.pointer_local() else {
            return;
        };
        self.dispatch_canvas(CanvasEvent::Move { local });
    }

    /// 滚轮（`precise = true` 为触控板像素增量）。
    pub(crate) fn dispatch_canvas_wheel(&mut self, delta_x: f32, delta_y: f32, precise: bool) {
        if !self.canvas_input_active() {
            return;
        }
        let Some(local) = self.pointer_local() else {
            return;
        };
        self.dispatch_canvas(CanvasEvent::Wheel {
            local,
            delta_x,
            delta_y,
            precise,
        });
    }

    /// 修饰键。
    pub(crate) fn dispatch_canvas_modifiers(&mut self, modifiers: winit::keyboard::ModifiersState) {
        if !self.canvas_input_active() {
            return;
        }
        use winit::keyboard::ModifiersState;
        self.dispatch_canvas(CanvasEvent::Modifiers {
            shift: modifiers.contains(ModifiersState::SHIFT),
            ctrl: modifiers.contains(ModifiersState::CONTROL),
        });
    }

    /// 窗口失焦：清掉可能收不到 Released 的修饰键。
    pub(crate) fn dispatch_canvas_focus_lost(&mut self) {
        if !self.canvas_input_active() {
            return;
        }
        self.dispatch_canvas(CanvasEvent::FocusLost);
    }

    /// 用编辑器当前状态刷新窗口光标形态。
    ///
    /// 复用 iced 侧的 `mouse_interaction` 判定（同一份语义），映射到 winit 图标。
    /// 指针落在 egui 占用区（菜单 / 面板 / 滚动条）时不改：那些区域有自己的光标。
    fn refresh_canvas_cursor(&mut self) {
        let canvas = &self.root.editor.editor_state.canvas;
        let Some(position) = self.window_ctx.cursor_position else {
            return;
        };
        if self
            .egui_blocked
            .iter()
            .any(|rect| rect.contains(egui::pos2(position.x, position.y)))
        {
            return;
        }
        let bounds = iced_core::Rectangle::new(
            iced_core::Point::new(canvas.offset_x, canvas.offset_y),
            iced_core::Size::new(canvas.size_x, canvas.size_y),
        );
        let cursor = iced_core::mouse::Cursor::Available(position);
        let interaction = if self.root.sidebar.is_vertical_roll() {
            lumino_ui_editor::grid::canvas_input::vertical::cursor(
                &self.root.editor,
                &self.canvas_input,
            )
        } else {
            lumino_ui_editor::grid::program_impl::mouse_interaction::handle(
                &self.root.editor,
                &self.canvas_input,
                bounds,
                cursor,
            )
        };
        self.apply_cursor_interaction(interaction);
    }

    /// 应用 egui 请求的光标（仅当指针落在 egui 占用区内，避免覆盖画布光标）。
    pub(crate) fn apply_egui_cursor(&mut self, icon: egui::CursorIcon) {
        let Some(position) = self.window_ctx.cursor_position else {
            return;
        };
        if !self
            .egui_blocked
            .iter()
            .any(|rect| rect.contains(egui::pos2(position.x, position.y)))
        {
            return;
        }
        use winit::window::CursorIcon;
        let icon = match icon {
            egui::CursorIcon::Default => CursorIcon::Default,
            egui::CursorIcon::PointingHand => CursorIcon::Pointer,
            egui::CursorIcon::Grab => CursorIcon::Grab,
            egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
            egui::CursorIcon::ResizeHorizontal => CursorIcon::EwResize,
            egui::CursorIcon::ResizeVertical => CursorIcon::NsResize,
            egui::CursorIcon::ResizeNwSe => CursorIcon::NwseResize,
            egui::CursorIcon::ResizeNeSw => CursorIcon::NeswResize,
            egui::CursorIcon::Text => CursorIcon::Text,
            egui::CursorIcon::Crosshair => CursorIcon::Crosshair,
            egui::CursorIcon::NotAllowed | egui::CursorIcon::NoDrop => CursorIcon::NotAllowed,
            egui::CursorIcon::Wait => CursorIcon::Wait,
            egui::CursorIcon::Progress => CursorIcon::Progress,
            egui::CursorIcon::Help => CursorIcon::Help,
            egui::CursorIcon::Move => CursorIcon::Move,
            egui::CursorIcon::AllScroll => CursorIcon::AllScroll,
            _ => CursorIcon::Default,
        };
        if self.window_ctx.applied_cursor != Some(Some(icon)) {
            self.window_ctx.window.set_cursor(icon);
            self.window_ctx.window.set_cursor_visible(true);
            self.window_ctx.applied_cursor = Some(Some(icon));
        }
    }
}
