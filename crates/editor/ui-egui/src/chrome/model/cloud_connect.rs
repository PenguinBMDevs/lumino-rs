//! 云存储连接面板模型。

/// 云连接面板：标题 / 协议 / 表单 / 错误 / 隐私提示 / 按钮。
#[derive(Debug, Clone, Default)]
pub struct CloudConnectModel {
    /// 顶部是否有 30px 旧版 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「连接云存储」。
    pub title: String,
    /// 「协议」标签。
    pub protocol_label: String,
    /// 三个协议选项。
    pub protocols: Vec<CloudProtocolOptionModel>,
    /// 五个输入行（名称 / 地址 / 端口 / 用户名 / 密码）。
    pub fields: Vec<CloudFieldModel>,
    /// 连接失败提示（「连接失败：{e}」）。
    pub error: Option<String>,
    /// 隐私提示（本地存储）。
    pub privacy: String,
    /// 正在连接（按钮置灰 + 文案变化）。
    pub connecting: bool,
    /// 连接按钮文案（「连接」/「连接中...」）。
    pub connect: String,
    /// 取消按钮。
    pub cancel: String,
}

/// 云存储协议（与生产 `CloudProtocolUi` 一一对应，本层不依赖消息 crate）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CloudProtocol {
    /// FTP（默认）。
    #[default]
    Ftp,
    /// SFTP。
    Sftp,
    /// WebDAV。
    Webdav,
}

/// 协议选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudProtocolOptionModel {
    /// 协议枚举。
    pub protocol: CloudProtocol,
    /// 显示名（FTP / SFTP / WebDAV）。
    pub label: String,
    /// 是否为当前选择。
    pub selected: bool,
}

/// 表单行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudFieldModel {
    /// 字段种类（动作回传时按此映射）。
    pub field: CloudField,
    /// 标签（协议 / 名称 / 地址 / 端口 / 用户名 / 密码）。
    pub label: String,
    /// 输入框占位。
    pub placeholder: String,
    /// 当前文本。
    pub value: String,
    /// 密码框（掩码显示）。
    pub password: bool,
}

/// 云连接表单字段（与生产 `CloudAction` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudField {
    /// 名称（可选）。
    Name,
    /// 服务器地址。
    Address,
    /// 端口（可选）。
    Port,
    /// 用户名。
    Username,
    /// 密码（可选）。
    Password,
}
