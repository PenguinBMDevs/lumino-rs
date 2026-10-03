//! 颜料桶「分音符填充」对话框处理器（主窗口覆盖层）

use crate::message::{FillDivisionAction, Message};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_fill_division(
        &self,
        root: &mut Root,
        action: FillDivisionAction,
    ) -> Option<Message> {
        match action {
            FillDivisionAction::OpenDialog => {
                let current = root.editor.fill_division();
                root.state.fill_division_dialog.is_open = true;
                root.state.fill_division_dialog.value =
                    current.map(|n| n.to_string()).unwrap_or_default();
            }
            FillDivisionAction::CloseDialog => {
                root.state.fill_division_dialog.is_open = false;
            }
            FillDivisionAction::ValueChanged(value) => {
                // 只允许数字（含空串）：与自定义精度对话框同口径，
                // 非法字符直接丢弃，输入框保持上一次合法值。
                if value.chars().all(|c| c.is_ascii_digit()) {
                    root.state.fill_division_dialog.value = value;
                }
            }
            FillDivisionAction::Confirm => {
                match root.state.fill_division_dialog.parse() {
                    Ok(division) => {
                        root.editor.set_fill_division(division);
                        root.state.fill_division_dialog.is_open = false;
                        tracing::info!("颜料桶: 填充切分档位 = {division:?}");
                    }
                    Err(e) => {
                        // 非法输入：保持对话框打开，让用户改（不静默丢弃）
                        tracing::warn!(
                            "颜料桶: 分音符输入非法 {:?}（{e}）",
                            root.state.fill_division_dialog.value
                        );
                    }
                }
            }
        }
        None
    }
}
