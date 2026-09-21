//! 云存储 UI 动作处理器
//!
//! 处理 `Message::Cloud(CloudAction)`：
//! - 更新 CloudUiState（表单输入、导航状态）
//! - 需要 runner 执行的操作（连接/列目录/下载/保存/断开）
//!   转换为 `crate::event::Event::Cloud` 发射到全局事件缓冲

mod connection;
mod file_ops;

use crate::event::{self, cloud as cloud_event};
use crate::message::Message;
use crate::root::Root;

use lumino_message::CloudAction;

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
}
