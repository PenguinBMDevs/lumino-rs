//! 对话框窗口 - 重绘

use super::DialogWindow;

impl DialogWindow {
    /// 请求重绘
    pub fn redraw(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) {
            match gfx.with_frame(|frame, view| ui.redraw_requested(frame, view, gfx)) {
                Ok(_) => {}
                Err(_) => {
                    self.window.request_redraw();
                }
            }
        }
    }

    /// 强制重绘：先标记 UI 脏确保 `view()` 重新构建，再执行正常 `redraw`。
    ///
    /// 用于内存监控等需要每帧重新捕获快照的对话框，避免 `render_iced_ui`
    /// 因 `ui_dirty == false` 进入「仅 present 缓存」的早退路径而跳过
    /// `UserInterface::build`（从而跳过 `view()` 中重新捕获 Snapshot）。
    pub fn redraw_force(&mut self) {
        if let Some(ui) = self.ui.as_mut() {
            ui.mark_dirty();
        }
        self.redraw();
    }
}
