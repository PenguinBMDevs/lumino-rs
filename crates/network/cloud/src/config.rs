//! 云存储连接配置持久化
//!
//! 配置文件：`{config_dir}/cloud.json`（与应用配置 config.json 同一文件夹）。
//! 地址/用户名明文存储，密码为 AES-256-GCM 密文（见 `crypto` 模块）。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CloudError, Result};
use crate::model::CloudConnection;

/// 配置文件根结构
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudConfigFile {
    /// 全部已保存的连接
    #[serde(default)]
    pub connections: Vec<CloudConnection>,
}

/// 配置存储封装（持有路径 + 内存副本）
#[derive(Debug)]
pub struct CloudConfigStore {
    inner: CloudConfigFile,
    path: std::path::PathBuf,
}

impl CloudConfigStore {
    /// 从指定路径加载配置（文件不存在或解析失败时回退空配置，
    /// 与主配置 ConfigWrapper 的容错行为一致）
    pub fn new(path: std::path::PathBuf) -> Result<Self> {
        let inner = if path.exists() {
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("读取云配置失败 {}: {e}，使用空配置", path.display());
                    return Ok(Self {
                        inner: CloudConfigFile::default(),
                        path,
                    });
                }
            };
            match serde_json::from_slice::<CloudConfigFile>(&bytes) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("解析云配置失败 {}: {e}，使用空配置", path.display());
                    CloudConfigFile::default()
                }
            }
        } else {
            CloudConfigFile::default()
        };
        Ok(Self { inner, path })
    }

    /// 全部连接
    pub fn connections(&self) -> &[CloudConnection] {
        &self.inner.connections
    }

    /// 按 ID 查找连接
    pub fn find(&self, id: &str) -> Option<&CloudConnection> {
        self.inner.connections.iter().find(|c| c.id == id)
    }

    /// 新增或更新连接（按 ID 匹配），并立即落盘
    pub fn upsert(&mut self, conn: CloudConnection) -> Result<()> {
        if let Some(existing) = self.inner.connections.iter_mut().find(|c| c.id == conn.id) {
            *existing = conn;
        } else {
            self.inner.connections.push(conn);
        }
        self.save()
    }

    /// 删除连接并落盘
    pub fn remove(&mut self, id: &str) -> Result<()> {
        let before = self.inner.connections.len();
        self.inner.connections.retain(|c| c.id != id);
        if self.inner.connections.len() == before {
            return Err(CloudError::Config(format!("连接不存在: {id}")));
        }
        self.save()
    }

    /// 落盘（JSON 美化格式）
    pub fn save(&self) -> Result<()> {
        save_config_to(&self.path, &self.inner)
    }
}

/// 将配置写入指定路径（原子写：先写临时文件再重命名，避免写坏配置）
pub fn save_config_to(path: &Path, config: &CloudConfigFile) -> Result<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| CloudError::Config(format!("序列化配置失败: {e}")))?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| CloudError::Config(format!("创建配置目录失败: {e}")))?;
    }

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes())
        .map_err(|e| CloudError::Config(format!("写入配置失败: {e}")))?;
    fs::rename(&tmp, path).map_err(|e| CloudError::Config(format!("保存配置失败: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CloudProtocol;

    fn test_conn(name: &str) -> CloudConnection {
        CloudConnection::new(
            name.into(),
            CloudProtocol::Ftp,
            "ftp.example.com".into(),
            None,
            "user".into(),
            crate::crypto::encrypt("secret").expect("加密应成功"),
            String::new(),
        )
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lumino-cloud-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn test_new_returns_empty_when_missing() {
        let path = temp_path("missing.json");
        let _ = fs::remove_file(&path);
        let store = CloudConfigStore::new(path.clone()).expect("加载应成功");
        assert!(store.connections().is_empty());
    }

    #[test]
    fn test_upsert_adds_and_updates() {
        let path = temp_path("upsert.json");
        let _ = fs::remove_file(&path);
        let mut store = CloudConfigStore::new(path.clone()).expect("加载应成功");

        let conn = test_conn("连接A");
        store.upsert(conn.clone()).expect("新增应成功");
        assert_eq!(store.connections().len(), 1);

        // 更新同名连接（ID 相同）
        let mut updated = conn.clone();
        updated.name = "连接A改".into();
        store.upsert(updated.clone()).expect("更新应成功");
        assert_eq!(store.connections().len(), 1, "更新不应新增条目");
        assert_eq!(store.find(&conn.id).expect("应存在").name, "连接A改");
    }

    #[test]
    fn test_remove() {
        let path = temp_path("remove.json");
        let _ = fs::remove_file(&path);
        let mut store = CloudConfigStore::new(path.clone()).expect("加载应成功");
        let conn = test_conn("待删");
        store.upsert(conn.clone()).expect("新增应成功");
        store.remove(&conn.id).expect("删除应成功");
        assert!(store.connections().is_empty());
        assert!(store.remove(&conn.id).is_err(), "删除不存在的连接应报错");
    }

    #[test]
    fn test_save_and_reload_roundtrip() {
        let path = temp_path("roundtrip.json");
        let _ = fs::remove_file(&path);
        let mut store = CloudConfigStore::new(path.clone()).expect("加载应成功");
        store.upsert(test_conn("往返测试")).expect("新增应成功");

        // 重新加载，验证持久化
        let reloaded = CloudConfigStore::new(path.clone()).expect("重新加载应成功");
        assert_eq!(reloaded.connections().len(), 1);
        let loaded = &reloaded.connections()[0];
        assert_eq!(loaded.name, "往返测试");
        assert_eq!(loaded.address, "ftp.example.com");
        // 密码必须是密文，且可解密还原
        assert_ne!(loaded.password_encrypted, "secret", "文件不得存明文密码");
        assert_eq!(
            crate::crypto::decrypt(&loaded.password_encrypted).expect("解密应成功"),
            "secret"
        );
    }

    #[test]
    fn test_file_does_not_contain_plaintext_password() {
        let path = temp_path("noplain.json");
        let _ = fs::remove_file(&path);
        let mut store = CloudConfigStore::new(path.clone()).expect("加载应成功");
        store.upsert(test_conn("无明文")).expect("新增应成功");

        let raw = fs::read_to_string(&path).expect("读取应成功");
        assert!(!raw.contains("secret"), "配置文件不得包含明文密码");
    }
}
