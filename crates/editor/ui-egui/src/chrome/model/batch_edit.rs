//! 批量编辑对话框模型。

/// 批量编辑对话框：四行输入 + 格式说明 + 按钮。
#[derive(Debug, Clone, Default)]
pub struct BatchEditModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「批量编辑」。
    pub title: String,
    /// 「设置」小节标题。
    pub settings_title: String,
    /// 四行输入（力度 / 长度 / key / tick）。
    pub rows: Vec<BatchEditRowModel>,
    /// 「格式」小节标题。
    pub format_title: String,
    /// 格式说明行（运算符号 / 无符号数字语义）。
    pub format_hints: Vec<String>,
    /// 确定按钮。
    pub ok: String,
    /// 取消按钮。
    pub cancel: String,
}

/// 批量编辑的一行输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEditRowModel {
    /// 对应生产字段（动作回传时按此映射）。
    pub field: BatchEditFieldModel,
    /// 标签（「音符力度:」…）。
    pub label: String,
    /// 输入框占位符（「如: +20」…）。
    pub placeholder: String,
    /// 当前文本。
    pub value: String,
}

/// 批量编辑字段（按生产 `BatchEditField` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchEditFieldModel {
    /// 音符力度。
    Velocity,
    /// 音符长度。
    Gate,
    /// 音符 key 位置。
    Key,
    /// 音符 tick 位置。
    Tick,
}
