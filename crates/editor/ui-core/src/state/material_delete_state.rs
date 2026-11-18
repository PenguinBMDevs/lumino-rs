//! 素材删除确认对话框状态（独立 OS 窗口，DRAW-045）。
//!
//! 对话框自己的展示数据（素材下标与显示名）放这里；文件删除仍由主窗口
//! 处理——对话框只产出「确认 / 取消」。

/// 素材删除确认对话框状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterialDeleteDialogState {
    /// 对话框是否打开。
    pub is_open: bool,
    /// 待删除素材在右侧栏素材列表中的下标。
    pub index: usize,
    /// 素材显示名（打开时快照，列表刷新后仍能正确展示）。
    pub name: String,
}

impl MaterialDeleteDialogState {
    /// 创建一个空的对话框状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开对话框并注入待删除素材。
    pub fn open(&mut self, index: usize, name: String) {
        self.is_open = true;
        self.index = index;
        self.name = name;
    }

    /// 关闭对话框（不清空素材信息：误关后重新打开由调用方重新注入）。
    pub fn close(&mut self) {
        self.is_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_keeps_index_and_name_snapshot() {
        let mut state = MaterialDeleteDialogState::new();
        state.open(3, "钢琴音色.json".to_string());
        assert!(state.is_open);
        assert_eq!(state.index, 3);
        assert_eq!(state.name, "钢琴音色.json");
    }

    #[test]
    fn test_close_keeps_payload_for_reopen() {
        let mut state = MaterialDeleteDialogState::new();
        state.open(1, "鼓组".to_string());
        state.close();
        assert!(!state.is_open, "关闭后不得继续渲染");
        assert_eq!(state.index, 1, "关闭不清空素材信息");
    }
}
