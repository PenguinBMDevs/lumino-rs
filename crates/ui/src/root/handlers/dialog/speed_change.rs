//! 音符变速对话框处理器

use crate::message::{Message, SpeedChangeAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_speed_change(
        &self,
        root: &mut Root,
        action: SpeedChangeAction,
    ) -> Option<Message> {
        use SpeedChangeAction as S;
        match action {
            S::OpenDialog => {
                root.state.speed_change_dialog.is_open = true;
            }
            S::CloseDialog => {
                root.state.speed_change_dialog.is_open = false;
                root.state.dialog_result = Some(crate::host::DialogResult::Cancel);
            }
            S::Confirm => {
                if let Some(factor) = root.state.speed_change_dialog.parse_factor() {
                    root.toolbar.speed_factor = factor;
                    tracing::info!("Root: 速度因子已更新为 {}", factor);
                    root.state.dialog_result = Some(crate::host::DialogResult::SpeedChange { factor });
                    if !root
                        .editor
                        .editor_state
                        .interaction
                        .selected_notes
                        .is_empty()
                    {
                        let modified = root.editor.apply_speed_change(factor);
                        if modified > 0 {
                            tracing::info!("Root: 变速完成，修改了 {} 个音符", modified);
                            root.update_playback_notes();
                            root.editor.clear_notes_changed();
                        }
                    } else {
                        tracing::warn!("Root: 没有选中音符，不执行变速对话框的变速操作");
                    }
                } else {
                    tracing::warn!(
                        "Root: 无效的速度因子输入: {}",
                        root.state.speed_change_dialog.factor_input
                    );
                }
                root.state.speed_change_dialog.is_open = false;
            }
            S::FactorChanged(value) => {
                root.state.speed_change_dialog.factor_input = value;
            }
        }
        None
    }
}
