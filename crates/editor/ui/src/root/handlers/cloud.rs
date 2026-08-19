//! 云存储 UI 动作处理器
//!
//! 处理 `Message::Cloud(CloudAction)`：
//! - 更新 CloudUiState（表单输入、导航状态）
//! - 需要 runner 执行的操作（连接/列目录/下载/保存/断开）
//!   转换为 `crate::event::Event::Cloud` 发射到全局事件缓冲

use crate::event::{self, cloud as cloud_event};
use crate::message::Message;
use crate::root::Root;
use crate::state::cloud_state::CloudClipboard;

use lumino_message::{CloudAction, CloudProtocolUi};

/// 云存储消息处理器
pub struct CloudHandler;

impl CloudHandler {
    /// 创建处理器
    pub fn new() -> Self {
        Self
    }
}

impl Default for CloudHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl super::MessageHandler for CloudHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        if let Message::Cloud(action) = msg {
            root.handle_cloud_action(action);
            None
        } else {
            Some(msg)
        }
    }
}

impl Root {
    /// 处理云存储 UI 动作
    pub fn handle_cloud_action(&mut self, action: CloudAction) {
        match action {
            // ── 连接表单 ──
            CloudAction::ProtocolSelected(protocol) => {
                self.cloud.protocol = protocol;
            }
            CloudAction::NameChanged(name) => self.cloud.name = name,
            CloudAction::AddressChanged(address) => self.cloud.address = address,
            CloudAction::PortChanged(port) => self.cloud.port = port,
            CloudAction::UsernameChanged(username) => self.cloud.username = username,
            CloudAction::PasswordChanged(password) => self.cloud.password = password,
            CloudAction::ConnectCancel => {
                self.cloud.connect_error = None;
                self.cloud.connecting = false;
            }
            CloudAction::Connect => self.cloud_connect(),

            // ── 文件浏览 ──
            CloudAction::SelectStorage(id) => self.cloud_select_storage(id),
            CloudAction::EnterDir(path) => self.cloud_enter_dir(path),
            CloudAction::Back => self.cloud_back(),
            CloudAction::Refresh => {
                self.request_list_dir();
            }
            CloudAction::Download { path } => self.cloud_download(path),
            CloudAction::Disconnect(id) => {
                event::emit(event::Event::cloud(cloud_event::Event::DisconnectRequest(
                    id,
                )));
            }
            CloudAction::NewFolderInputChanged(name) => self.cloud.new_folder_input = name,
            CloudAction::NewFolder(name) => self.cloud_new_folder(name),
            CloudAction::SaveHere => self.cloud_save_here(),

            // ── 文件操作（复制/剪切/粘贴/重命名/删除） ──
            CloudAction::CopyEntry { path, is_dir } => self.cloud_copy_entry(path, is_dir),
            CloudAction::CutEntry { path, is_dir } => self.cloud_cut_entry(path, is_dir),
            CloudAction::Paste => self.cloud_paste(),
            CloudAction::ClearClipboard => {
                self.cloud.clipboard = None;
                self.cloud.notice = None;
            }
            CloudAction::RequestDelete { path, is_dir } => self.cloud_request_delete(path, is_dir),
            CloudAction::DeleteEntry { path, is_dir } => self.cloud_delete_entry(path, is_dir),
            CloudAction::DeleteCancel => {
                self.cloud.pending_delete = None;
                self.cloud.notice = None;
            }
            CloudAction::StartRename(path) => self.cloud_start_rename(path),
            CloudAction::RenameInputChanged(name) => {
                self.cloud.rename_input = name;
            }
            CloudAction::RenameConfirm => self.cloud_rename_confirm(),
            CloudAction::RenameCancel => {
                self.cloud.renaming = None;
                self.cloud.notice = None;
            }

            // ── 云管理（设置面板入口） ──
            CloudAction::OpenConnectPanel => {
                event::emit(event::Event::cloud(cloud_event::Event::OpenConnectPanel));
            }
            CloudAction::OpenBrowserPanel => {
                event::emit(event::Event::cloud(cloud_event::Event::OpenBrowserPanel {
                    intent: "import".to_string(),
                }));
            }
            CloudAction::ConnectExisting(id) => {
                self.cloud.connecting = true;
                event::emit(event::Event::cloud(cloud_event::Event::ConnectExisting {
                    id,
                }));
            }
            CloudAction::DeleteConnection(id) => {
                event::emit(event::Event::cloud(cloud_event::Event::DeleteConnection {
                    id,
                }));
            }
            CloudAction::DismissAlert => {
                self.cloud.alert_message = None;
                event::emit(event::Event::cloud(cloud_event::Event::DismissAlert));
            }
        }
    }

    /// 发起连接请求（校验必填项 + 发射 ConnectRequest 事件）
    fn cloud_connect(&mut self) {
        if self.cloud.connecting {
            return;
        }
        // 校验必填项
        if self.cloud.address.trim().is_empty() {
            self.cloud.connect_error = Some("服务器地址不能为空".to_string());
            return;
        }
        // 解析端口（非法输入回退默认）
        let port = self
            .cloud
            .port
            .trim()
            .parse::<u16>()
            .unwrap_or_else(|_| self.cloud.protocol.default_port());
        let protocol = self.cloud.protocol;

        self.cloud.connecting = true;
        self.cloud.connect_error = None;
        event::emit(event::Event::cloud(cloud_event::Event::ConnectRequest {
            name: if self.cloud.name.trim().is_empty() {
                default_conn_name(protocol, &self.cloud.address)
            } else {
                self.cloud.name.trim().to_string()
            },
            protocol: protocol.as_str().to_string(),
            address: self.cloud.address.trim().to_string(),
            port,
            username: self.cloud.username.clone(),
            password: self.cloud.password.clone(),
        }));
    }

    /// 选中云存储连接：重置浏览状态并列出根目录
    fn cloud_select_storage(&mut self, id: String) {
        if self.cloud.selected_id.as_deref() == Some(id.as_str()) {
            return;
        }
        self.cloud.selected_id = Some(id);
        self.cloud.current_path = String::new();
        self.cloud.entries.clear();
        self.cloud.notice = None;
        self.request_list_dir();
    }

    /// 进入子目录（防御：UI 只对目录行发此动作）
    fn cloud_enter_dir(&mut self, path: String) {
        self.cloud.current_path = path;
        self.cloud.entries.clear();
        self.request_list_dir();
    }

    /// 返回上一级目录
    fn cloud_back(&mut self) {
        let path = self.cloud.current_path.as_str();
        if path.is_empty() || path == "/" {
            self.cloud.current_path = String::new();
            return;
        }
        let trimmed = path.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(idx) => self.cloud.current_path = trimmed[..idx].to_string(),
            None => self.cloud.current_path = String::new(),
        }
        self.cloud.entries.clear();
        self.request_list_dir();
    }

    /// 下载远程文件（素材过滤器决定目标：素材库 or 导入目录）
    fn cloud_download(&mut self, path: String) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        let target = if self.cloud.filter.as_deref() == Some("lmmaterial") {
            cloud_event::DownloadTarget::Material
        } else {
            cloud_event::DownloadTarget::Import
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::DownloadRequest {
            id,
            remote_path: path,
            target,
        }));
    }

    /// 新建文件夹（校验名称 + 发射 NewFolderRequest 事件）
    fn cloud_new_folder(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.cloud.notice = Some("文件夹名称不能为空".to_string());
            return;
        }
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.new_folder_input.clear();
        event::emit(event::Event::cloud(cloud_event::Event::NewFolderRequest {
            id,
            parent: self.cloud.current_path.clone(),
            name,
        }));
    }

    /// 保存到当前目录：有素材上传待办 → 上传素材；否则上传工程归档
    fn cloud_save_here(&mut self) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        // 素材上传待办存在 → 上传素材文件（素材库右键"上传到云"）；
        // 否则上传当前工程归档（文件菜单"保存到云"）。
        if let Some(pending) = self.cloud.pending_upload.take() {
            event::emit(event::Event::cloud(
                cloud_event::Event::UploadMaterialRequest {
                    id,
                    dir_path: self.cloud.current_path.clone(),
                    local_path: pending.local_path,
                    file_name: pending.file_name,
                },
            ));
        } else {
            event::emit(event::Event::cloud(
                cloud_event::Event::SaveToCloudRequest {
                    id,
                    dir_path: self.cloud.current_path.clone(),
                },
            ));
        }
    }

    /// 复制条目到剪贴板（目录复制暂不支持）
    fn cloud_copy_entry(&mut self, path: String, is_dir: bool) {
        if is_dir {
            self.cloud.notice = Some("复制目录暂不支持，请使用剪切移动".to_string());
            return;
        }
        self.cloud.clipboard = Some(CloudClipboard::new(path.clone(), false, false));
        self.cloud.notice = Some(format!("已复制：{}", basename_of(&path)));
    }

    /// 剪切条目到剪贴板
    fn cloud_cut_entry(&mut self, path: String, is_dir: bool) {
        self.cloud.clipboard = Some(CloudClipboard::new(path.clone(), is_dir, true));
        self.cloud.notice = Some(format!("已剪切：{}", basename_of(&path)));
    }

    /// 粘贴剪贴板条目（复制/移动请求）
    fn cloud_paste(&mut self) {
        let Some(clip) = self.cloud.clipboard.clone() else {
            self.cloud.notice = Some("剪贴板为空".to_string());
            return;
        };
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        event::emit(event::Event::cloud(cloud_event::Event::CopyRequest {
            id,
            from: clip.source_path,
            to_dir: self.cloud.current_path.clone(),
            is_cut: clip.is_cut,
        }));
    }

    /// 请求删除：进入行内确认态（不立即删除）
    fn cloud_request_delete(&mut self, path: String, is_dir: bool) {
        self.cloud.pending_delete = Some((path.clone(), is_dir, basename_of(&path)));
        self.cloud.notice = None;
    }

    /// 确认删除（由行内确认态触发）
    fn cloud_delete_entry(&mut self, path: String, is_dir: bool) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.pending_delete = None;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::DeleteRequest {
            id,
            path,
            is_dir,
        }));
    }

    /// 开始重命名：进入行内编辑态
    fn cloud_start_rename(&mut self, path: String) {
        self.cloud.renaming = Some(path.clone());
        self.cloud.rename_input = basename_of(&path);
        self.cloud.notice = None;
    }

    /// 确认重命名：目标路径 = 源目录 + 新名称
    fn cloud_rename_confirm(&mut self) {
        let Some(from) = self.cloud.renaming.clone() else {
            return;
        };
        let new_name = self.cloud.rename_input.trim().to_string();
        if new_name.is_empty() {
            self.cloud.notice = Some("名称不能为空".to_string());
            return;
        }
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        // 目标路径 = 源目录 + 新名称
        let parent = match from.rfind('/') {
            Some(0) => String::new(),
            Some(idx) => from[..idx].to_string(),
            None => String::new(),
        };
        let to = if parent.is_empty() {
            format!("/{new_name}")
        } else {
            format!("{parent}/{new_name}")
        };
        self.cloud.renaming = None;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::RenameRequest {
            id,
            from,
            to,
        }));
    }

    /// 请求列出当前选中连接的当前目录
    fn request_list_dir(&mut self) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::ListDirRequest {
            id,
            path: self.cloud.current_path.clone(),
        }));
    }
}

/// 默认连接名称（用户未填时按协议+地址生成）
fn default_conn_name(protocol: CloudProtocolUi, address: &str) -> String {
    format!("{} - {}", protocol.display_name(), address)
}

/// 从远程完整路径提取条目名（与 cloud 客户端 basename 语义一致）
fn basename_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename_of() {
        assert_eq!(basename_of("/Moyingjun/file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("/file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("/Moyingjun/dir/"), "dir");
        assert_eq!(
            basename_of("/Moyingjun/Parallel Unit.lmpj"),
            "Parallel Unit.lmpj"
        );
    }
}
