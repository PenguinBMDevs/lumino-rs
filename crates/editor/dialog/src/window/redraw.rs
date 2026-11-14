//! 对话框窗口 - 重绘

use super::DialogWindow;

impl DialogWindow {
    /// 请求重绘
    pub fn redraw(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let egui_events = self.egui_input.take();
        let egui_focused = self.egui_input.focused();
        // REND-019（PR-A §1.6）：统一帧错误处置——Outdated 已重配 /
        // Timeout 下帧重试可重绘；Lost/OOM 置停帧标志（禁止盲重绘）
        if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut())
            && let Err(e) = gfx.with_frame(|frame, view| {
                ui.redraw_requested(frame, view, gfx, egui_events, egui_focused)
            })
            && gfx.handle_frame_error(&e).should_redraw()
        {
            self.window.request_redraw();
        }
        if let Some(ui) = self.ui.as_ref() {
            self.egui_input.set_wants_keyboard(ui.egui_wants_keyboard());
        }
    }

    /// 强制重绘：先标记 UI 脏确保 `view()` 重新构建，再执行正常 `redraw`。
    ///
    /// 用于内存监控等需要每帧重新捕获快照的对话框，避免 `render_ui`
    /// 因 `ui_dirty == false` 进入「仅 present 缓存」的早退路径而跳过
    /// `UserInterface::build`（从而跳过 `view()` 中重新捕获 Snapshot）。
    pub fn redraw_force(&mut self) {
        if let Some(ui) = self.ui.as_mut() {
            ui.mark_dirty();
        }
        self.redraw();
    }
}
