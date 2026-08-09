//! 云存储事件（UI → Runner 请求 / Runner → UI 结果）
//!
//! 方向约定：
//! - `*Request`：UI 请求 runner 执行云操作（连接/列目录/下载/保存/断开）
//! - `*Result`：runner 在后台线程执行完毕后的结果回传（经全局事件缓冲）

/// 云存储事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 打开云存储面板请求（入口分流：无连接 → 连接面板；有连接 → 浏览面板）
    OpenCloudPanel {
        /// 入口意图（"import"/"save"/"material"）
        intent: String,
    },
    /// 连接请求
    ConnectRequest {
        /// 显示名称
        name: String,
        /// 协议标识（"ftp"/"sftp"/"webdav"）
        protocol: String,
        /// 服务器地址
        address: String,
        /// 端口（0 = 协议默认）
        port: u16,
        /// 用户名
        username: String,
        /// 明文密码（仅内存传递，落盘前加密）
        password: String,
    },
    /// 断开连接请求
    DisconnectRequest(String),
    /// 列出目录请求
    ListDirRequest {
        /// 连接 ID
        id: String,
        /// 远程目录路径
        path: String,
    },
    /// 下载文件请求
    DownloadRequest {
        /// 连接 ID
        id: String,
        /// 远程文件路径
        remote_path: String,
        /// 入口类型（素材/工程/MIDI/其他）
        target: DownloadTarget,
    },
    /// 保存到云请求（上传当前工程归档到目标目录）
    SaveToCloudRequest {
        /// 连接 ID
        id: String,
        /// 目标目录
        dir_path: String,
    },
    /// 新建文件夹请求
    NewFolderRequest {
        /// 连接 ID
        id: String,
        /// 父目录
        parent: String,
        /// 新文件夹名称
        name: String,
    },
    /// 打开云连接面板（设置面板"添加连接"）
    OpenConnectPanel,
    /// 打开云文件浏览面板（设置面板"管理文件"）
    OpenBrowserPanel {
        /// 入口意图（"import"/"save"/"material"）
        intent: String,
    },
    /// 连接已保存的指定连接
    ConnectExisting {
        /// 连接 ID
        id: String,
    },
    /// 删除已保存的连接配置
    DeleteConnection {
        /// 连接 ID
        id: String,
    },
    /// 关闭断连提醒面板
    DismissAlert,
    /// 重命名请求
    RenameRequest {
        /// 连接 ID
        id: String,
        /// 原路径
        from: String,
        /// 新路径
        to: String,
    },
    /// 删除请求
    DeleteRequest {
        /// 连接 ID
        id: String,
        /// 路径
        path: String,
        /// 是否为目录
        is_dir: bool,
    },
    /// 移动请求
    MoveRequest {
        /// 连接 ID
        id: String,
        /// 源路径
        from: String,
        /// 目标目录
        to_dir: String,
    },

    // ── 结果回传（Runner → UI） ──
    /// 连接结果
    ConnectResult {
        /// 成功后的连接 ID（失败为空）
        id: String,
        /// 是否成功
        ok: bool,
        /// 错误原因（失败时）
        error: Option<String>,
    },
    /// 目录列表结果
    ListDirResult {
        /// 连接 ID
        id: String,
        /// 请求的目录路径
        path: String,
        /// 条目列表（成功时）
        entries: Vec<RemoteEntry>,
        /// 错误原因
        error: Option<String>,
    },
    /// 下载结果
    DownloadResult {
        /// 远程路径
        remote_path: String,
        /// 是否成功
        ok: bool,
        /// 错误原因（失败时）
        error: Option<String>,
        /// 下载到的本地路径（成功时）
        local_path: Option<String>,
    },
    /// 保存到云结果
    SaveToCloudResult {
        /// 是否成功
        ok: bool,
        /// 错误原因
        error: Option<String>,
    },
    /// 通用操作结果（新建/重命名/删除/移动/断开）
    OperationResult {
        /// 是否成功
        ok: bool,
        /// 错误原因
        error: Option<String>,
    },
}

/// 下载入口类型（决定下载后的处理方式）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadTarget {
    /// 素材库入口：仅 .lmmaterial，下载到用户素材目录
    Material,
    /// 文件菜单入口：MIDI 导入工程，其余下载到本地
    Import,
}

/// 远程条目（结果回传用，轻量结构）
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    /// 条目名称
    pub name: String,
    /// 远程完整路径
    pub path: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件大小（字节）
    pub size: u64,
}
