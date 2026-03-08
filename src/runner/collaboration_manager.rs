/**
 * 协作对话框管理
 * 
 * 用于显示连接服务器、创建房间、加入房间等UI
 */

use std::sync::Arc;
use tokio::sync::Mutex;

pub mod connect_dialog;
pub mod room_dialog;

pub use connect_dialog::ConnectDialog;
pub use room_dialog::RoomDialog;

/// 对话框结果
#[derive(Debug, Clone)]
pub enum CollaborationDialogResult {
    Connect { host: String, port: u16, username: String },
    CreateRoom { name: String },
    JoinRoom { invite_code: String },
    Disconnect,
    Cancel,
}

/// 协作管理器
pub struct CollaborationManager {
    client: Option<Arc<Mutex<lumino_collaboration::CollaborationClient>>>,
}

impl CollaborationManager {
    pub fn new() -> Self {
        Self { client: None }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub fn client(&self) -> Option<&Arc<Mutex<lumino_collaboration::CollaborationClient>>> {
        self.client.as_ref()
    }

    pub fn set_client(&mut self, client: Arc<Mutex<lumino_collaboration::CollaborationClient>>) {
        self.client = Some(client);
    }

    pub fn disconnect(&mut self) {
        self.client = None;
    }
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}
