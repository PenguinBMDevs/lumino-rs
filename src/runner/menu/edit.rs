//! Runner 编辑菜单事件处理

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理编辑菜单事件
    ///
    /// **P1-5 修复**：剪切 / 复制 / 粘贴 / 全选的**视图仲裁已下沉到 UI 层**
    /// （`lumino_ui::Host::handle_edit_menu_action`）。本函数只负责把菜单事件
    /// 递进去——Runner 不该知道「钢琴卷帘 / 工程走带」这种视图细节，也不该
    /// 在此处硬编码 `EditorAction`（那正是「键盘通、菜单不通」的成因：
    /// 键盘路径在 `host/event/keyboard.rs` 有走带分支，菜单路径原先没有）。
    pub(super) fn handle_edit_menu_event(&mut self, event: lumino_ui::event::menu::edit::Event) {
        let ui = self.window_state.window.ui_mut();
        ui.handle_edit_menu_action(event);
    }
}
