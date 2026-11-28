//! 历史入栈：push_history 与带 OpKind 的版本。

use crate::Editor;

impl Editor {
    /// Push current state to history
    pub fn push_history(&mut self) {
        self.editor_state.data.push_history();
    }

    /// Push current state to history with an explicit op kind
    ///
    /// 形状参数提交（新建/改参数/删除/组合/区域着色/遮罩实体化）应使用
    /// `OpKind::ShapeEdit`，让历史面板显示「形状编辑」而不是「其他操作」。
    pub fn push_history_with_op_kind(&mut self, op_kind: lumino_note_core::history::OpKind) {
        self.editor_state.data.push_history_with_op_kind(op_kind);
    }
}
