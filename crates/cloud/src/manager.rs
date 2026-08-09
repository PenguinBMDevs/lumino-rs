//! 云存储管理器 — 连接池 + 配置 + 状态 + 操作分发
//!
//! `CloudManager` 持有 tokio Runtime，所有方法为**同步阻塞**接口，
//! 内部通过 `block_on` 执行异步协议客户端。调用方应将耗时操作
//! 放到后台线程执行（如 runner 中的 std::thread::spawn），避免阻塞 UI。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::runtime::Runtime;

use crate::client::{CloudClient, create_client};
use crate::config::CloudConfigStore;
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry, ConnState};

/// 云存储管理器
pub struct CloudManager {
    rt: Runtime,
    config: CloudConfigStore,
    clients: HashMap<String, Box<dyn CloudClient>>,
    status: HashMap<String, ConnState>,
}

impl CloudManager {
    /// 创建管理器（加载配置 + 创建异步运行时）
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let rt = match Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return Err(CloudError::Operation(format!("创建异步运行时失败: {e}"))),
        };
        Ok(Self {
            rt,
            config: CloudConfigStore::new(config_path)?,
            clients: HashMap::new(),
            status: HashMap::new(),
        })
    }

    // ── 配置管理 ──

    /// 当前全部连接配置（快照）
    pub fn connections(&self) -> &[CloudConnection] {
        self.config.connections()
    }

    /// 按 ID 查找连接配置
    pub fn find_connection(&self, id: &str) -> Option<&CloudConnection> {
        self.config.find(id)
    }

    /// 新增或更新连接配置（已连接的同 ID 连接先断开）
    pub fn upsert_connection(&mut self, conn: CloudConnection) -> Result<()> {
        self.disconnect(&conn.id);
        self.config.upsert(conn)
    }

    /// 删除连接配置
    pub fn remove_connection(&mut self, id: &str) -> Result<()> {
        self.disconnect(id);
        self.config.remove(id)
    }

    /// 当前连接状态
    pub fn status(&self, id: &str) -> ConnState {
        self.status
            .get(id)
            .cloned()
            .unwrap_or(ConnState::Disconnected)
    }

    /// 全部已连接（在线）的连接 ID
    pub fn online_ids(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    // ── 连接管理 ──

    /// 连接指定 ID 的云存储（阻塞）
    pub fn connect(&mut self, id: &str) -> Result<()> {
        let conn = self
            .config
            .find(id)
            .cloned()
            .ok_or_else(|| CloudError::Config(format!("连接不存在: {id}")))?;
        self.status.insert(id.to_string(), ConnState::Connecting);

        let mut client = create_client(conn.protocol)?;
        let result = self.rt.block_on(client.connect(&conn));
        match result {
            Ok(()) => {
                self.clients.insert(id.to_string(), client);
                self.status.insert(id.to_string(), ConnState::Online);
                tracing::info!("云存储已连接: {} ({})", conn.name, conn.protocol);
                Ok(())
            }
            Err(e) => {
                self.status
                    .insert(id.to_string(), ConnState::Failed(e.to_string()));
                tracing::warn!("云存储连接失败: {} ({}) : {e}", conn.name, conn.protocol);
                Err(e)
            }
        }
    }

    /// 自动连接全部标记 auto_connect 的连接（阻塞，逐个尝试）
    pub fn connect_all_auto(&mut self) -> Vec<(String, Result<()>)> {
        let ids: Vec<String> = self
            .config
            .connections()
            .iter()
            .filter(|c| c.auto_connect)
            .map(|c| c.id.clone())
            .collect();
        ids.iter()
            .map(|id| (id.clone(), self.connect(id)))
            .collect()
    }

    /// 断开指定连接
    pub fn disconnect(&mut self, id: &str) {
        if let Some(mut client) = self.clients.remove(id) {
            let _ = self.rt.block_on(client.disconnect());
        }
        self.status.insert(id.to_string(), ConnState::Disconnected);
        tracing::info!("云存储已断开: {id}");
    }

    // ── 文件操作（阻塞，内部 block_on） ──

    /// 列出目录内容
    pub fn list_dir(&mut self, id: &str, path: &str) -> Result<Vec<CloudEntry>> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.list_dir(path).await
        });
        self.handle_io_result(id, result)
    }

    /// 上传本地文件到远程路径
    pub fn upload(&mut self, id: &str, local: &Path, remote_path: &str) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.upload_file(local, remote_path).await
        });
        self.handle_io_result(id, result)
    }

    /// 下载远程文件到本地路径
    pub fn download(&mut self, id: &str, remote_path: &str, local: &Path) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.download_file(remote_path, local).await
        });
        self.handle_io_result(id, result)
    }

    /// 重命名（同目录内）
    pub fn rename(&mut self, id: &str, from: &str, to: &str) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.rename(from, to).await
        });
        self.handle_io_result(id, result)
    }

    /// 删除文件或目录
    pub fn delete(&mut self, id: &str, path: &str, is_dir: bool) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.delete(path, is_dir).await
        });
        self.handle_io_result(id, result)
    }

    /// 移动文件/目录到目标目录（云内部）
    pub fn move_file(&mut self, id: &str, from: &str, to_dir: &str) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.move_file(from, to_dir).await
        });
        self.handle_io_result(id, result)
    }

    /// 创建目录
    pub fn create_dir(&mut self, id: &str, path: &str) -> Result<()> {
        let result = self.rt.block_on(async {
            let client = self
                .clients
                .get_mut(id)
                .ok_or_else(|| CloudError::NotConnected(format!("云存储未连接: {id}")))?;
            client.create_dir(path).await
        });
        self.handle_io_result(id, result)
    }

    /// 处理 IO 结果：失败时更新连接状态为 Failed（断连检测）
    fn handle_io_result<T>(&mut self, id: &str, result: Result<T>) -> Result<T> {
        match &result {
            Ok(_) => {
                // 恢复在线状态（若此前标记过 Failed）
                self.status.insert(id.to_string(), ConnState::Online);
            }
            Err(e) => {
                // 网络类错误视为断连：记录状态，由 UI 层决定是否提醒
                tracing::warn!("云存储操作失败 {id}: {e}");
                self.status
                    .insert(id.to_string(), ConnState::Failed(e.to_string()));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encrypt;
    use crate::model::CloudProtocol;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumino-cloud-mgr-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn test_new_loads_empty_config() {
        let path = temp_path("mgr-empty.json");
        let _ = std::fs::remove_file(&path);
        let mgr = CloudManager::new(path).expect("创建管理器应成功");
        assert!(mgr.connections().is_empty());
    }

    #[test]
    fn test_upsert_and_remove_connection() {
        let path = temp_path("mgr-config.json");
        let _ = std::fs::remove_file(&path);
        let mut mgr = CloudManager::new(path.clone()).expect("创建管理器应成功");

        let conn = CloudConnection::new(
            "测试连接".into(),
            CloudProtocol::Webdav,
            "example.com".into(),
            None,
            "user".into(),
            encrypt("pass").expect("加密应成功"),
            String::new(),
        );
        mgr.upsert_connection(conn.clone()).expect("新增应成功");
        assert_eq!(mgr.connections().len(), 1);
        assert_eq!(
            mgr.status(&conn.id),
            ConnState::Disconnected,
            "新增连接不应自动连接"
        );

        mgr.remove_connection(&conn.id).expect("删除应成功");
        assert!(mgr.connections().is_empty());
    }

    #[test]
    fn test_connect_missing_id_returns_err() {
        let path = temp_path("mgr-missing.json");
        let _ = std::fs::remove_file(&path);
        let mut mgr = CloudManager::new(path).expect("创建管理器应成功");
        assert!(mgr.connect("nonexistent").is_err());
    }

    #[test]
    fn test_connect_failure_records_status() {
        let path = temp_path("mgr-fail.json");
        let _ = std::fs::remove_file(&path);
        let mut mgr = CloudManager::new(path).expect("创建管理器应成功");

        // 127.0.0.1:1 端口必然拒绝连接 → 应快速失败并记录 Failed 状态
        let conn = CloudConnection::new(
            "失败测试".into(),
            CloudProtocol::Ftp,
            "127.0.0.1".into(),
            Some(1),
            "user".into(),
            encrypt("pass").expect("加密应成功"),
            String::new(),
        );
        mgr.upsert_connection(conn.clone()).expect("新增应成功");

        let result = mgr.connect(&conn.id);
        assert!(result.is_err(), "连接未监听端口应失败");
        assert!(
            matches!(mgr.status(&conn.id), ConnState::Failed(_)),
            "失败后状态应为 Failed"
        );
        assert!(mgr.online_ids().is_empty(), "失败连接不应出现在在线列表");
    }

    #[test]
    fn test_disconnect_unknown_id_is_noop() {
        let path = temp_path("mgr-disconnect.json");
        let _ = std::fs::remove_file(&path);
        let mut mgr = CloudManager::new(path).expect("创建管理器应成功");
        mgr.disconnect("unknown");
        assert_eq!(mgr.status("unknown"), ConnState::Disconnected);
    }
}
