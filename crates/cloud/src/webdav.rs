//! WebDAV 客户端实现（基于 reqwest_dav）
//!
//! 路径规范：本模块内统一使用**相对路径**（如 `/dav/dir/file.txt`），
//! 因为 reqwest_dav 内部以 `host + path` 拼接请求 URL，传完整 URL 会出错。
//! 服务器返回的 href 通过 `normalize_href` 规范化为相对路径。

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest_dav::types::Auth;
use reqwest_dav::types::list_cmd::ListEntity;
use reqwest_dav::{Client as DavClient, ClientBuilder, Depth};

use crate::client::{CloudClient, basename, join_remote, normalize_remote};
use crate::crypto::decrypt;
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry};

/// 连接/请求超时（秒）
const CONNECT_TIMEOUT_SECS: u64 = 10;
const REQUEST_TIMEOUT_SECS: u64 = 60;

/// WebDAV 客户端
pub struct WebdavClient {
    client: Option<DavClient>,
    /// 服务器 host（完整 URL，如 `https://example.com/dav`）
    host: String,
}

impl WebdavClient {
    /// 创建空客户端
    pub fn new() -> Self {
        Self {
            client: None,
            host: String::new(),
        }
    }
}

/// 将服务器返回的 href 规范化为相对路径
///
/// - 完整 URL（含 `scheme://`）→ 提取 path 部分
/// - 相对路径 → 原样返回（去除查询参数与 fragment）
fn normalize_href(href: &str) -> String {
    let cleaned = href.split(['?', '#']).next().unwrap_or(href);
    if let Some(pos) = cleaned.find("://") {
        let after = &cleaned[pos + 3..];
        match after.find('/') {
            Some(slash) => {
                let path = &after[slash..];
                if path.is_empty() {
                    "/".to_string()
                } else {
                    path.to_string()
                }
            }
            None => "/".to_string(),
        }
    } else {
        cleaned.to_string()
    }
}

/// 判断条目是否为目录自身（PROPFIND depth=1 会包含自身条目）
fn is_self(href: &str, path: &str) -> bool {
    let href = normalize_href(href);
    href.trim_end_matches('/') == path.trim_end_matches('/')
}

/// 将 reqwest_dav 错误转换为 CloudError
fn dav_err(e: reqwest_dav::types::Error) -> CloudError {
    CloudError::Protocol(e.to_string())
}

#[async_trait]
impl CloudClient for WebdavClient {
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()> {
        let password = decrypt(&conn.password_encrypted)?;
        let mut host = conn.address.trim().to_string();
        if host.is_empty() {
            return Err(CloudError::Config("WebDAV 地址不能为空".into()));
        }
        // 地址未携带协议时默认补 http://
        if !host.contains("://") {
            host = format!("http://{host}");
        }

        let agent = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| CloudError::Connect(format!("创建 HTTP 客户端失败: {e}")))?;

        let auth = if conn.username.is_empty() {
            Auth::Anonymous
        } else {
            Auth::Basic(conn.username.clone(), password)
        };

        let client = ClientBuilder::new()
            .set_agent(agent)
            .set_host(host.clone())
            .set_auth(auth)
            .build()
            .map_err(dav_err)?;

        self.client = Some(client);
        self.host = host;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client.take();
        Ok(())
    }

    async fn list_dir(&mut self, path: &str) -> Result<Vec<CloudEntry>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        // 统一规范化为绝对路径（消除 ./ 残留）
        let target = normalize_remote(path);

        let entities = client
            .list(&target, Depth::Number(1))
            .await
            .map_err(dav_err)?;
        let mut entries = Vec::new();
        for entity in entities {
            match entity {
                ListEntity::File(f) => {
                    if is_self(&f.href, &target) {
                        continue;
                    }
                    let remote = normalize_href(&f.href);
                    entries.push(CloudEntry {
                        name: basename(&remote),
                        path: normalize_remote(&remote),
                        is_dir: false,
                        size: f.content_length.max(0) as u64,
                        modified: Some(f.last_modified.timestamp().max(0) as u64),
                    });
                }
                ListEntity::Folder(f) => {
                    if is_self(&f.href, &target) {
                        continue;
                    }
                    let remote = normalize_href(&f.href);
                    entries.push(CloudEntry {
                        name: basename(&remote),
                        path: normalize_remote(&remote),
                        is_dir: true,
                        size: 0,
                        modified: Some(f.last_modified.timestamp().max(0) as u64),
                    });
                }
            }
        }
        Ok(entries)
    }

    async fn upload_file(&mut self, local: &Path, remote_path: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let bytes = std::fs::read(local).map_err(CloudError::Io)?;
        let remote_path = normalize_remote(remote_path);
        client
            .put(&remote_path, bytes)
            .await
            .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let remote_path = normalize_remote(remote_path);
        let resp = client
            .get(&remote_path)
            .await
            .map_err(|e| CloudError::Protocol(format!("下载失败 {remote_path}: {e}")))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CloudError::Protocol(format!("读取下载数据失败: {e}")))?;
        std::fs::write(local, &bytes).map_err(CloudError::Io)?;
        Ok(())
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let from = normalize_remote(from);
        let to = normalize_remote(to);
        client
            .mv(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("重命名失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn delete(&mut self, path: &str, _is_dir: bool) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let path = normalize_remote(path);
        client
            .delete(&path)
            .await
            .map_err(|e| CloudError::Protocol(format!("删除失败 {path}: {e}")))?;
        Ok(())
    }

    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let from = normalize_remote(from);
        let to = normalize_remote(join_remote(to_dir, &basename(&from)).as_str());
        client
            .mv(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("移动失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn create_dir(&mut self, path: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let path = normalize_remote(path);
        client
            .mkcol(&path)
            .await
            .map_err(|e| CloudError::Protocol(format!("创建目录失败 {path}: {e}")))?;
        Ok(())
    }
}

impl Default for WebdavClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_href_full_url() {
        assert_eq!(
            normalize_href("http://example.com/dav/file.txt"),
            "/dav/file.txt"
        );
        assert_eq!(normalize_href("https://example.com/dav/dir/"), "/dav/dir/");
        assert_eq!(normalize_href("http://example.com"), "/");
    }

    #[test]
    fn test_normalize_href_relative() {
        assert_eq!(normalize_href("/dav/file.txt"), "/dav/file.txt");
        assert_eq!(normalize_href("file.txt"), "file.txt");
        assert_eq!(
            normalize_href("/dav/file.txt?query=1#frag"),
            "/dav/file.txt"
        );
    }

    #[test]
    fn test_is_self() {
        assert!(is_self("/dav/dir", "/dav/dir"));
        assert!(is_self("/dav/dir/", "/dav/dir"));
        assert!(!is_self("/dav/dir/file.txt", "/dav/dir"));
        assert!(!is_self("/dav/other", "/dav/dir"));
    }

    #[test]
    fn test_host_default_scheme() {
        // 地址规范化逻辑（connect 内联，此处验证思路）
        let raw = "example.com:8080/dav";
        let host = if raw.contains("://") {
            raw.to_string()
        } else {
            format!("http://{raw}")
        };
        assert_eq!(host, "http://example.com:8080/dav");
    }
}
