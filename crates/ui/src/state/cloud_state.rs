//! 云存储 UI 状态
//!
//! 仅保存展示所需状态（表单输入、连接列表快照、目录条目）；
//! 实际连接与文件操作由 runner 在后台线程执行，结果通过
//! `Host::set_cloud_state` 注入本状态。

use lumino_message::CloudProtocolUi;

/// 云存储 UI 状态
#[derive(Debug, Clone, Default)]
pub struct CloudUiState {
    // ── 连接表单（CloudConnect 面板） ──
    /// 协议
    pub protocol: CloudProtocolUi,
    /// 显示名称
    pub name: String,
    /// 服务器地址
    pub address: String,
    /// 端口（空 = 协议默认）
    pub port: String,
    /// 用户名
    pub username: String,
    /// 密码（仅内存，不持久化）
    pub password: String,
    /// 正在连接（连接期间禁用提交按钮）
    pub connecting: bool,
    /// 连接错误提示（失败时显示原因）
    pub connect_error: Option<String>,

    // ── 文件浏览（CloudBrowser 面板） ──
    /// 已保存的连接快照（设备下拉列表）
    pub connections: Vec<CloudConnInfo>,
    /// 当前选中的连接 ID
    pub selected_id: Option<String>,
    /// 当前目录路径
    pub current_path: String,
    /// 当前目录条目
    pub entries: Vec<CloudEntryUi>,
    /// 操作进行中（列表加载/下载/保存时禁用按钮）
    pub busy: bool,
    /// 操作结果提示（成功/失败信息）
    pub notice: Option<String>,
    /// 文件类型过滤（扩展名，如 "lmmaterial"；None = 全部）
    pub filter: Option<String>,
    /// 保存模式（true = 面板用于"保存到云"目标选择）
    pub save_mode: bool,
    /// 新建文件夹输入框内容
    pub new_folder_input: String,
    /// 断连提醒内容（CloudNotice 面板与设置面板标志共用）
    pub alert_message: Option<String>,
}

impl CloudUiState {
    /// 当前是否有在线连接
    pub fn has_online(&self) -> bool {
        self.connections.iter().any(|c| c.online)
    }

    /// 当前选中连接的显示信息
    pub fn selected(&self) -> Option<&CloudConnInfo> {
        let id = self.selected_id.as_ref()?;
        self.connections.iter().find(|c| &c.id == id)
    }
}

/// 云连接信息（设备下拉项）
#[derive(Debug, Clone)]
pub struct CloudConnInfo {
    /// 连接 ID
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 协议显示名
    pub protocol: String,
    /// 是否在线
    pub online: bool,
}

/// 远程条目（列表展示）
#[derive(Debug, Clone)]
pub struct CloudEntryUi {
    /// 条目名称
    pub name: String,
    /// 远程路径
    pub path: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（Unix 秒，未知为 None）
    pub modified: Option<u64>,
}

/// 格式化文件大小（B/KB/MB/GB）
pub fn format_size(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size = size as f64;
    if size >= GB {
        format!("{:.2} GB", size / GB)
    } else if size >= MB {
        format!("{:.2} MB", size / MB)
    } else if size >= KB {
        format!("{:.1} KB", size / KB)
    } else {
        format!("{size:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.00 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn test_has_online_and_selected() {
        let mut state = CloudUiState::default();
        assert!(!state.has_online());
        assert!(state.selected().is_none());

        state.connections.push(CloudConnInfo {
            id: "a".into(),
            name: "A".into(),
            protocol: "FTP".to_string(),
            online: false,
        });
        assert!(!state.has_online());

        state.connections.push(CloudConnInfo {
            id: "b".into(),
            name: "B".into(),
            protocol: "SFTP".to_string(),
            online: true,
        });
        assert!(state.has_online());

        state.selected_id = Some("b".into());
        assert_eq!(state.selected().map(|c| c.name.as_str()), Some("B"));
    }
}
