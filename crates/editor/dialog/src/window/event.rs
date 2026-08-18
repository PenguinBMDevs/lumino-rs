//! 对话框窗口 - 事件处理与窗口动作

use lumino_ui::host::DialogResult;
use lumino_ui::window::TrafficAction;
use winit::event::WindowEvent;

use super::DialogWindow;

impl DialogWindow {
    /// 处理窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        match &event {
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) {
                    ui.resize(size.width, size.height);
                    gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::CloseRequested => {
                self.should_close = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ui) = self.ui.as_mut() {
                    ui.cursor_moved(*position);
                }
            }
            _ => {}
        }

        if let Some(ui) = self.ui.as_mut() {
            ui.handle_events(event, winit::keyboard::ModifiersState::default());
        }
    }

    /// 应用自制标题栏产生的窗口动作（关闭 / 最小化 / 拖动）
    ///
    /// 无边框模式下，系统标题栏已移除，窗口控制完全由 iced 渲染的自制
    /// 标题栏（`Titlebar`）承担：红绿灯按钮与拖动区通过 `window::Event`
    /// 消息，由 `Host::process_message` 写入 `pending_window_action` /
    /// `pending_drag`。这些动作必须在 `redraw()`（事件队列已被消费）之后
    /// 取出并应用到真实的 winit 窗口。
    ///
    /// 与 `WindowManager::handle_window_actions` 的关键差异：对话框的 `Close`
    /// **不会退出事件循环**，仅标记 `should_close`，由 runner 负责关闭该对话框。
    pub fn apply_window_actions(&mut self) {
        let (action, drag) = match self.ui.as_mut() {
            Some(ui) => (ui.take_window_action(), ui.take_drag()),
            None => (None, false),
        };

        if let Some(action) = action {
            match action {
                TrafficAction::Close => {
                    // 关闭对话框（不退出整个事件循环）
                    self.should_close = true;
                }
                TrafficAction::Minimize => {
                    self.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    // 不可缩放的对话框无需响应最大化
                    if self.window.is_resizable() {
                        let is_max = self.window.is_maximized();
                        self.window.set_maximized(!is_max);
                    }
                }
            }
        }

        if drag && let Err(e) = self.window.drag_window() {
            tracing::warn!("拖动对话框窗口失败: {}", e);
            if let Some(ui) = self.ui.as_mut() {
                ui.release_left_mouse_button();
            }
        }
    }

    /// 检查并获取对话框结果（需在 handle_event 之后调用）
    pub fn check_result(&mut self) -> Option<DialogResult> {
        if let Some(ui) = self.ui.as_mut()
            && let Some(result) = ui.take_dialog_result()
        {
            // 永久删除音轨缓存后对话框保持开启（Runner 会刷新条目列表，支持连续操作）；
            // 其余结果沿用"取到结果即关闭"的默认行为。
            let keep_open = matches!(&result, DialogResult::RecoverTrackPermanentlyDelete { .. });
            self.result_data = Some(result);
            if !keep_open {
                self.should_close = true;
            }
            return self.result_data.take();
        }
        None
    }
}
