//! Runner 视图菜单处理

use crate::runner::RunnerInner;

/// 缩放比例因子
const ZOOM_FACTOR: f32 = 1.2;
/// X轴默认缩放
const DEFAULT_ZOOM_X: f32 = 0.1;
/// Y轴默认缩放
const DEFAULT_ZOOM_Y: f32 = 20.0;

impl RunnerInner {
    /// 处理视图菜单事件
    pub(super) fn handle_view_menu_event(
        &mut self,
        view_event: lumino_ui::event::menu::view::Event,
    ) {
        use lumino_ui::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
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
            ZoomIn => {
                let ui = self.window_state.window.ui_mut();
                let root = ui.root_mut();
                let new_zoom_x = root.editor.editor_state.view.zoom_x * ZOOM_FACTOR;
                let new_zoom_y = root.editor.editor_state.view.zoom_y * ZOOM_FACTOR;
                root.editor.set_zoom_x(new_zoom_x, 0.5);
                root.editor.set_zoom_y(new_zoom_y, 0.5);
            }
            ZoomOut => {
                let ui = self.window_state.window.ui_mut();
                let root = ui.root_mut();
                let new_zoom_x = root.editor.editor_state.view.zoom_x / ZOOM_FACTOR;
                let new_zoom_y = root.editor.editor_state.view.zoom_y / ZOOM_FACTOR;
                root.editor.set_zoom_x(new_zoom_x, 0.5);
                root.editor.set_zoom_y(new_zoom_y, 0.5);
            }
            ZoomReset => {
                let ui = self.window_state.window.ui_mut();
                let root = ui.root_mut();
                root.editor.set_zoom_x(DEFAULT_ZOOM_X, 0.5);
                root.editor.set_zoom_y(DEFAULT_ZOOM_Y, 0.5);
            }
        }
    }
}
