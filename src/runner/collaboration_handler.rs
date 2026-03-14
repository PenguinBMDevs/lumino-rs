use std::sync::Arc;
use tokio::sync::Mutex;

/// 协作状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationStatus {
    #[default]
    Disconnected,
}

/// 协作处理器
#[derive(Clone)]
pub struct CollaborationHandler {
    client: Option<Arc<Mutex<lumino_collaboration::CollaborationClient>>>,
    status: CollaborationStatus,
}

impl CollaborationHandler {
    pub fn new() -> Self {
        Self {
            client: None,
            status: CollaborationStatus::default(),
        }
    }

    /// 创建房间
    pub fn create_room(&self, name: String) -> Result<(), String> {
        tracing::info!("协作: 请求创建房间 - {}", name);
        if let Some(client) = &self.client {
            let client = client.clone();
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.create_room(name) {
                    tracing::error!("协作: 创建房间失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err("协作客户端未初始化".to_string())
        }
    }

    /// 加入房间
    pub fn join_room(&self, invite_code: String) -> Result<(), String> {
        tracing::info!("协作: 请求加入房间 - {}", invite_code);
        if let Some(client) = &self.client {
            let client = client.clone();
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.join_room(invite_code) {
                    tracing::error!("协作: 加入房间失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err("协作客户端未初始化".to_string())
        }
    }

    /// 断开连接
    pub async fn disconnect(&mut self) -> Result<(), String> {
        tracing::info!("协作: 请求断开连接");
        if let Some(client) = self.client.take() {
            tokio::spawn(async move {
                let mut c = client.lock().await;
                if let Err(e) = c.disconnect().await {
                    tracing::error!("协作: 断开连接失败: {}", e);
                }
            });
        }
        self.status = CollaborationStatus::Disconnected;
        Ok(())
    }
}
