//! 撤销历史对话框模型。

/// 撤销历史对话框（只读时间线 + 跳转）。
#[derive(Debug, Clone, Default)]
pub struct HistoryModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「撤销历史」。
    pub title: String,
    /// 提示语。
    pub hint: String,
    /// 行（初始状态 + 每个步骤，最老在上）。
    pub rows: Vec<HistoryRowModel>,
    /// 底部计数「current / total」。
    pub counter: String,
    /// 关闭按钮。
    pub close: String,
}

/// 历史面板的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRowModel {
    /// 跳转目标（0 = 初始状态）。
    pub index: usize,
    /// 行文案（已本地化）。
    pub label: String,
    /// 是否为当前游标所在行。
    pub current: bool,
    /// 是否为「未来」步骤（浅色）。
    pub future: bool,
}
