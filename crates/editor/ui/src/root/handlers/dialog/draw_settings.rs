//! 「音符画设置」对话框处理器
//!
//! - 主窗侧 `OpenDialog`：触发 Runner 打开独立 OS 窗口（`DialogType::DrawSettings`）。
//! - 对话框侧：本轮为占位面板，唯一出口是「关闭」→ `DialogResult::Cancel`
//!   （由 Runner 收尾关闭窗口，与画刷对话框的取消路径同源）。

use lumino_message::DrawSettingsAction;

use crate::host::DialogResult;
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_draw_settings(
        &self,
        root: &mut Root,
        action: DrawSettingsAction,
    ) -> Option<crate::message::Message> {
        match action {
            DrawSettingsAction::OpenDialog => {
                // 主窗侧：请求打开独立 OS 对话框。
                tracing::info!("Root: 请求打开音符画设置对话框");
                crate::event::emit(crate::event::Event::Window(
                    crate::event::window::Event::open_draw_settings_dialog(),
                ));
            }
            DrawSettingsAction::CloseDialog => {
                tracing::info!("音符画设置: 关闭对话框");
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
        }
        None
    }
}
