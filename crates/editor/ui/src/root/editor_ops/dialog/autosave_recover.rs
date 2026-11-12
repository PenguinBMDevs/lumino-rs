//! 自动保存恢复对话框结果处理（DEBT-12）
//!
//! - 恢复：交给 Runner 按「未保存工程」载入候选文件；
//! - 不恢复：交给 Runner 把文件改名保留（`*-declined-*`）。
//!
//! Root 只负责产出结果，不碰文件系统——恢复语义全部在 Runner 的
//! `auto_save/recovery.rs` 里，便于脱离 winit 测试。

use crate::message::AutosaveRecoverAction;
use crate::root::Root;

impl Root {
    /// 自动保存恢复对话框动作
    pub(crate) fn handle_autosave_recover(&mut self, action: AutosaveRecoverAction) {
        self.state.dialog_result = Some(crate::host::DialogResult::AutosaveRecover(action));
        self.state.autosave_recover_dialog.is_open = false;
    }
}
