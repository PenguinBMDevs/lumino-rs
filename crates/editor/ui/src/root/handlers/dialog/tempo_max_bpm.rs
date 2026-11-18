//! 自定义 Tempo BPM 上限对话框处理器（DRAW-045，独立 OS 窗口）。
//!
//! 校验规则在 `TempoMaxBpmDialogState::parse_bpm`：非法输入保留窗口并写
//! `error`，有效输入产出 `DialogResult::TempoMaxBpm` 由 Runner 应用。

use crate::host::DialogResult;
use crate::message::{Message, TempoMaxBpmDialogAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_tempo_max_bpm(
        &self,
        root: &mut Root,
        action: TempoMaxBpmDialogAction,
    ) -> Option<Message> {
        match action {
            TempoMaxBpmDialogAction::Input(value) => {
                root.state.tempo_max_bpm_dialog.input = value;
                root.state.tempo_max_bpm_dialog.error = None;
            }
            TempoMaxBpmDialogAction::Confirm => {
                let dialog = &mut root.state.tempo_max_bpm_dialog;
                match dialog.parse_bpm() {
                    Ok(bpm) => {
                        dialog.error = None;
                        dialog.close();
                        root.state.dialog_result = Some(DialogResult::TempoMaxBpm { bpm });
                        tracing::info!("自定义 BPM 上限已提交：{bpm}，等待应用");
                    }
                    Err(error) => {
                        tracing::warn!("自定义 BPM 上限输入非法：{error}");
                        dialog.error = Some(error);
                    }
                }
            }
            TempoMaxBpmDialogAction::Cancel => {
                root.state.tempo_max_bpm_dialog.close();
                root.state.dialog_result = Some(DialogResult::Cancel);
                tracing::info!("自定义 BPM 上限：已取消");
            }
        }
        None
    }
}
