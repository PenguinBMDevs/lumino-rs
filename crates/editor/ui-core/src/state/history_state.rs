//! 撤销历史面板状态（DRAW-035）
//!
//! 与 `OpKind` 解耦：Runner/UI 把编辑器的历史投影成简单枚举，
//! 独立对话框 Root 不需要依赖 editor-state / note-core。

/// 历史步骤类型（文案映射在 UI 视图层）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// 新建音符
    NoteCreate,
    /// 移动音符
    NoteMove,
    /// 删除音符
    NoteDelete,
    /// 变换音符
    NoteTransform,
    /// 调整力度
    VelocityEdit,
    /// 自动化编辑
    AutomationEdit,
    /// MIDI 录制
    Recording,
    /// 其他 / 未分类
    Other,
}

/// 历史面板的一步
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryStepView {
    /// 跳到该步后的历史游标（1-based；0 = 初始状态）
    pub index: usize,
    /// 操作类型
    pub kind: HistoryKind,
}

/// 历史面板状态
#[derive(Debug, Clone, Default)]
pub struct HistoryDialogState {
    /// 全部步骤（最老在上）
    pub steps: Vec<HistoryStepView>,
    /// 当前已执行步数（0..=steps.len()）
    pub current: usize,
}

impl HistoryDialogState {
    /// 用编辑器快照替换面板数据；返回是否有变化（避免每帧重复重绘）。
    pub fn replace(&mut self, steps: Vec<HistoryStepView>, current: usize) -> bool {
        if self.steps == steps && self.current == current {
            return false;
        }
        self.steps = steps;
        self.current = current;
        true
    }
}
