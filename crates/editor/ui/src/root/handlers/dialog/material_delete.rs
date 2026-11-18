//! 素材删除确认对话框处理器（DRAW-045，独立 OS 窗口）。
//!
//! 对话框 Root 只产出 `DialogResult`；真正的文件删除与列表刷新由 Runner
//! 分派回主窗口完成（见 `Host::apply_material_delete`）。

use crate::host::DialogResult;
use crate::message::{MaterialDeleteDialogAction, Message};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_material_delete(
        &self,
        root: &mut Root,
        action: MaterialDeleteDialogAction,
    ) -> Option<Message> {
        match action {
            MaterialDeleteDialogAction::Confirm(index) => {
                root.state.dialog_result = Some(DialogResult::MaterialDeleteConfirm { index });
                tracing::info!("素材删除确认：下标 {index}，等待 Runner 分派");
            }
            MaterialDeleteDialogAction::Cancel => {
                root.state.dialog_result = Some(DialogResult::Cancel);
                tracing::info!("素材删除确认：已取消");
            }
        }
        root.state.material_delete_dialog.close();
        None
    }
}
