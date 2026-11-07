//! 设置「云管理」页的只读模型。
//!
//! 连接行携带稳定 id，按钮动作按 id 回传；告警文本与所有标签由生产侧预计算。

/// 云管理页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CloudPageModel {
    /// 标题。
    pub title: String,
    /// 断连提醒（已格式化；`None` = 不显示）。
    pub alert: Option<String>,
    /// 「添加连接」按钮文本。
    pub add_label: String,
    /// 无连接时的提示。
    pub empty_text: String,
    /// 连接行。
    pub rows: Vec<CloudConnModel>,
}

/// 单个云连接行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudConnModel {
    /// 连接 id（动作回传）。
    pub id: String,
    /// 名称 + 协议（`name [protocol]`）。
    pub title: String,
    /// 服务器地址。
    pub address: String,
    /// 状态标记（🟢 在线 / 🔴 离线）。
    pub status: String,
    /// 是否在线。
    pub online: bool,
    /// 主按钮文本（连接 / 断开）。
    pub connect_label: String,
    /// 「管理文件」文本。
    pub manage_label: String,
    /// 「删除」文本。
    pub delete_label: String,
}
