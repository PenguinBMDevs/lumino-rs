//! Runner 视图菜单处理

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理视图菜单事件
    pub(super) fn handle_view_menu_event(
        &mut self,
        view_event: lumino_core::event::menu::view::Event,
    ) {
        use lumino_core::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.window.ui_mut().update_theme(theme.clone());
                self.storage.config.patch(|state| {
                    state.ui.theme = theme;
                });
            }
        }
    }
}
