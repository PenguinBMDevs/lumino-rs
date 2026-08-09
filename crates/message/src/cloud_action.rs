//! 云存储 UI 动作消息

/// 云存储协议（UI 层轻量枚举，避免 UI 依赖 lumino-cloud 全量协议实现）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudProtocolUi {
    #[default]
    Ftp,
    Sftp,
    Webdav,
}

impl CloudProtocolUi {
    /// 协议显示名称
    pub fn display_name(self) -> &'static str {
        match self {
            CloudProtocolUi::Ftp => "FTP",
            CloudProtocolUi::Sftp => "SFTP",
            CloudProtocolUi::Webdav => "WebDAV",
        }
    }

    /// 协议标识字符串（与 runner/event 层通信）
    pub fn as_str(self) -> &'static str {
        match self {
            CloudProtocolUi::Ftp => "ftp",
            CloudProtocolUi::Sftp => "sftp",
            CloudProtocolUi::Webdav => "webdav",
        }
    }

    /// 默认端口
    pub fn default_port(self) -> u16 {
        match self {
            CloudProtocolUi::Ftp => 21,
            CloudProtocolUi::Sftp => 22,
            CloudProtocolUi::Webdav => 80,
        }
    }
}

impl std::fmt::Display for CloudProtocolUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// 云存储 UI 动作
///
/// 由云对话框渲染触发；需要跨进程执行的动作（连接/列目录/下载等）
/// 由处理器转换为 `lumino_ui::event::Event::Cloud` 交给 runner 执行。
#[derive(Debug, Clone)]
pub enum CloudAction {
    // ── 连接表单 ──
    /// 选择协议
    ProtocolSelected(CloudProtocolUi),
    /// 显示名称变更
    NameChanged(String),
    /// 地址变更
    AddressChanged(String),
    /// 端口变更
    PortChanged(String),
    /// 用户名变更
    UsernameChanged(String),
    /// 密码变更
    PasswordChanged(String),
    /// 提交连接（表单数据 → runner 执行连接）
    Connect,
    /// 取消连接面板
    ConnectCancel,

    // ── 文件浏览面板 ──
    /// 切换当前存储设备（连接 ID）
    SelectStorage(String),
    /// 进入目录
    EnterDir(String),
    /// 返回上级目录
    Back,
    /// 刷新当前目录
    Refresh,
    /// 下载文件到本地（远程路径）
    Download { path: String },
    /// 断开指定连接（退出登录）
    Disconnect(String),
    /// 新建文件夹输入框内容变更
    NewFolderInputChanged(String),
    /// 新建文件夹（输入名称）
    NewFolder(String),
    /// 保存到此处（保存模式：上传当前工程归档到当前目录）
    SaveHere,

    // ── 云管理（设置面板入口） ──
    /// 打开云连接面板（设置面板"添加连接"）
    OpenConnectPanel,
    /// 打开云文件浏览面板（设置面板"管理文件"）
    OpenBrowserPanel,
    /// 连接已保存的指定连接
    ConnectExisting(String),
    /// 删除已保存的连接配置
    DeleteConnection(String),
    /// 关闭断连提醒面板
    DismissAlert,
}
