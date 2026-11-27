//! Runner 视图菜单处理

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 全局应用主题：主窗口 + 所有已打开对话框，并更新配置持久化缓存。
    ///
    /// View 菜单与设置面板「切换即全局生效」共用同一链路，
    /// 配置实际落盘由随后一轮的 `save_storage` 完成。
    pub(crate) fn apply_global_theme(&mut self, theme: String) {
        self.window_state
            .window
            .ui_mut()
            .update_theme(theme.clone());
        // 同步所有已打开对话框窗口的主题（对话框主题为创建时快照）
        self.window_state
            .dialog_manager
            .update_theme_all(theme.clone());
        self.window_state.storage.config.patch(|state| {
            state.ui.theme = theme;
        });
    }

    /// 处理视图菜单事件
    pub(super) fn handle_view_menu_event(
        &mut self,
        view_event: lumino_ui::event::menu::view::Event,
    ) {
        use lumino_ui::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.apply_global_theme(theme);
            }
            ZoomIn => {
                self.window_state.window.ui_mut().root_mut().zoom_in();
            }
            ZoomOut => {
                self.window_state.window.ui_mut().root_mut().zoom_out();
            }
            ZoomReset => {
                self.window_state.window.ui_mut().root_mut().zoom_reset();
            }
        }
    }
}
