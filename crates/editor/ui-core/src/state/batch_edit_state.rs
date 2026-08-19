//! 批量编辑对话框状态

pub use lumino_note_core::batch_edit::{BatchEditOperation, parse_batch_edit_input};

/// 批量编辑对话框状态
#[derive(Debug, Clone)]
pub struct BatchEditDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 音符力度输入
    pub velocity_input: String,
    /// 音符长度输入
    pub gate_input: String,
    /// 音符 key 位置输入
    pub key_input: String,
    /// 音符 tick 位置输入
    pub tick_input: String,
}

impl BatchEditDialogState {
    /// 创建一个默认的批量编辑对话框状态
    pub fn new() -> Self {
        Self {
            is_open: false,
            velocity_input: String::new(),
            gate_input: String::new(),
            key_input: String::new(),
            tick_input: String::new(),
        }
    }
}

impl Default for BatchEditDialogState {
    fn default() -> Self {
        Self::new()
    }
}
