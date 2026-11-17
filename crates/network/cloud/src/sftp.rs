//! SFTP 客户端实现（基于 russh + russh-sftp，纯 Rust）
//!
//! 支持密码认证。russh 全程异步，无需阻塞线程包装；
//! 由 `CloudManager` 持有的 tokio Runtime 通过 `block_on` 执行。

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use russh::client::{AuthResult, Config as SshConfig, Handle, Handler, connect};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{check_known_hosts, check_known_hosts_path};
use russh_sftp::client::SftpSession;

use lumino_core::{tr, trf};

use crate::client::{CloudClient, basename, join_remote, normalize_remote, resolve_remote};
use crate::crypto::decrypt;
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry};

/// 服务器主机密钥校验处理器。
///
/// 使用 OpenSSH 的 `~/.ssh/known_hosts`；未知主机与密钥变更都会在密码认证前拒绝。
struct SshHandler {
    host: String,
    port: u16,
    known_hosts: Option<std::path::PathBuf>,
}

impl SshHandler {
    fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            known_hosts: None,
        }
    }
}

fn verify_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
    known_hosts: Option<&Path>,
) -> std::result::Result<bool, russh::keys::Error> {
    match known_hosts {
        Some(path) => check_known_hosts_path(host, port, server_public_key, path),
        None => check_known_hosts(host, port, server_public_key),
    }
}

impl Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint(HashAlg::Sha256);
        match verify_host_key(
            &self.host,
            self.port,
            server_public_key,
            self.known_hosts.as_deref(),
        ) {
            Ok(true) => Ok(true),
            Ok(false) => {
                tracing::warn!(
                    "拒绝未知 SFTP 主机 {}:{}（SHA-256 指纹 {}）；请先将主机密钥加入 ~/.ssh/known_hosts",
                    self.host,
                    self.port,
                    fingerprint
                );
                Ok(false)
            }
            Err(e) => {
                tracing::warn!(
                    "拒绝 SFTP 主机 {}:{}（SHA-256 指纹 {}）：主机密钥校验失败: {e}",
                    self.host,
                    self.port,
                    fingerprint
                );
                Err(e.into())
            }
        }
    }
}

/// SFTP 客户端
pub struct SftpClient {
    session: Option<Handle<SshHandler>>,
    sftp: Option<SftpSession>,
    /// 浏览根基准目录（用户视角 `/` 对应的服务器真实路径）。
    /// SFTP 无"当前目录"概念，所有操作必须用绝对路径；
    /// 用户配置的 root_path 或默认 `/` 即为浏览根。
    base: String,
}

impl SftpClient {
    /// 创建空客户端
    pub fn new() -> Self {
        Self {
            session: None,
            sftp: None,
            base: "/".to_string(),
        }
    }
}

/// 将 SFTP 错误转换为 CloudError
fn sftp_err(e: impl std::fmt::Display) -> CloudError {
    CloudError::Protocol(e.to_string())
}

#[async_trait]
impl CloudClient for SftpClient {
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()> {
        let password = decrypt(&conn.password_encrypted)?;
        let address = conn.address.clone();
        let port = conn.effective_port();
        let username = conn.username.clone();
        let root_path = conn.root_path.clone();

        // DEBT-06 #123：空闲/保活超时——旧实现 `SshConfig::default()` 无超时，
        // 服务器假死会永久挂起在通道 IO 上。
        let ssh_config = SshConfig {
            inactivity_timeout: Some(std::time::Duration::from_secs(120)),
            keepalive_interval: Some(std::time::Duration::from_secs(30)),
            ..SshConfig::default()
        };
        let config = Arc::new(ssh_config);
        let handler = SshHandler::new(address.clone(), port);
        let mut session = connect(config, (address.as_str(), port), handler)
            .await
            .map_err(|e| {
                CloudError::Connect(trf!(
                    "无法连接 {address}:{port}: {e}",
                    address = address,
                    port = port,
                    e = e
                ))
            })?;

        match session.authenticate_password(&username, &password).await {
            Ok(AuthResult::Success) => {}
            Ok(_) => return Err(CloudError::Auth(tr!("SFTP 认证未通过").to_string())),
            Err(e) => return Err(CloudError::Auth(trf!("SFTP 认证失败: {e}", e = e))),
        }

        // 打开 SFTP 子系统
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| CloudError::Protocol(trf!("打开会话通道失败: {e}", e = e)))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| CloudError::Protocol(trf!("请求 SFTP 子系统失败: {e}", e = e)))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| CloudError::Protocol(trf!("SFTP 初始化失败: {e}", e = e)))?;

        // 校验根目录可访问
        let base = if root_path.is_empty() {
            "/".to_string()
        } else {
            normalize_remote(&root_path)
        };
        sftp.metadata(&base).await.map_err(|e| {
            CloudError::Protocol(trf!("进入根目录失败 {base}: {e}", base = base, e = e))
        })?;

        self.session = Some(session);
        self.sftp = Some(sftp);
        self.base = base;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(sftp) = self.sftp.take() {
            let _ = sftp.close().await;
        }
        self.session.take();
        Ok(())
    }

    async fn list_dir(&mut self, path: &str) -> Result<Vec<CloudEntry>> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        // 用户视角路径（/xxx）→ 服务器真实路径（base + 相对段）
        let target = resolve_remote(&self.base, path);

        let entries = sftp.read_dir(&target).await.map_err(sftp_err)?;
        let mut result = Vec::new();
        for entry in entries {
            let name = entry.file_name();
            if matches!(name.as_str(), "." | "..") {
                continue;
            }
            let meta = entry.metadata();
            result.push(CloudEntry {
                name: name.clone(),
                // 返回用户视角路径（相对浏览根），不暴露服务器真实结构
                path: normalize_remote(&join_remote(path, &name)),
                is_dir: meta.is_dir(),
                size: meta.size.unwrap_or(0),
                modified: meta.mtime.map(|t| t as u64),
            });
        }
        Ok(result)
    }

    async fn upload_file(&mut self, local: &Path, remote_path: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let bytes = std::fs::read(local).map_err(CloudError::Io)?;
        let remote_path = resolve_remote(&self.base, remote_path);
        // 必须用 create（CREATE|TRUNCATE|WRITE）而非 write：
        // session::write 只带 WRITE 标志，OpenSSH sftp-server 对 WRITE-only
        // 打开不存在的文件返回 NoSuchFile——上传新文件必然失败。
        use tokio::io::AsyncWriteExt;
        let mut file = sftp.create(&remote_path).await.map_err(|e| {
            CloudError::Protocol(trf!(
                "上传失败 {remote_path}: {e}",
                remote_path = remote_path,
                e = e
            ))
        })?;
        file.write_all(&bytes).await.map_err(|e| {
            CloudError::Protocol(trf!(
                "上传失败 {remote_path}: {e}",
                remote_path = remote_path,
                e = e
            ))
        })?;
        // 等待全部写入确认与 CLOSE 完成，确保数据落盘
        file.close().await.map_err(|e| {
            CloudError::Protocol(trf!(
                "上传失败 {remote_path}: {e}",
                remote_path = remote_path,
                e = e
            ))
        })?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let remote_path = resolve_remote(&self.base, remote_path);
        let bytes = sftp.read(&remote_path).await.map_err(|e| {
            CloudError::Protocol(trf!(
                "下载失败 {remote_path}: {e}",
                remote_path = remote_path,
                e = e
            ))
        })?;
        std::fs::write(local, &bytes).map_err(CloudError::Io)?;
        Ok(())
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, to);
        sftp.rename(&from, &to).await.map_err(|e| {
            CloudError::Protocol(trf!(
                "重命名失败 {from} → {to}: {e}",
                from = from,
                to = to,
                e = e
            ))
        })?;
        Ok(())
    }

    async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let path = resolve_remote(&self.base, path);
        if is_dir {
            sftp.remove_dir(&path).await.map_err(|e| {
                CloudError::Protocol(trf!("删除目录失败 {path}: {e}", path = path, e = e))
            })?;
        } else {
            sftp.remove_file(&path).await.map_err(|e| {
                CloudError::Protocol(trf!("删除文件失败 {path}: {e}", path = path, e = e))
            })?;
        }
        Ok(())
    }

    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, join_remote(to_dir, &basename(&from)).as_str());
        sftp.rename(&from, &to).await.map_err(|e| {
            CloudError::Protocol(trf!(
                "移动失败 {from} → {to}: {e}",
                from = from,
                to = to,
                e = e
            ))
        })?;
        Ok(())
    }

    async fn create_dir(&mut self, path: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected(tr!("SFTP 未连接").to_string()))?;
        let path = resolve_remote(&self.base, path);
        sftp.create_dir(&path).await.map_err(|e| {
            CloudError::Protocol(trf!("创建目录失败 {path}: {e}", path = path, e = e))
        })?;
        Ok(())
    }
}

impl Default for SftpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::parse_public_key_base64;

    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

    fn public_key(value: &str) -> PublicKey {
        parse_public_key_base64(value).expect("测试公钥应可解析")
    }

    fn known_hosts_fixture(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lumino-cloud-sftp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建测试目录应成功");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("写入 known_hosts 夹具应成功");
        path
    }

    #[test]
    fn test_known_host_key_is_accepted() {
        let path = known_hosts_fixture(
            "known-hosts-accepted",
            &format!("[localhost]:2222 ssh-ed25519 {KEY_A}\n"),
        );
        let accepted = verify_host_key("localhost", 2222, &public_key(KEY_A), Some(&path))
            .expect("已登记密钥应通过校验");
        assert!(accepted);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_unknown_host_key_is_rejected() {
        let path = known_hosts_fixture(
            "known-hosts-unknown",
            &format!("[localhost]:2222 ssh-ed25519 {KEY_A}\n"),
        );
        let accepted = verify_host_key("example.com", 22, &public_key(KEY_A), Some(&path))
            .expect("未知主机应返回未登记状态");
        assert!(!accepted);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_changed_host_key_is_rejected() {
        let path = known_hosts_fixture(
            "known-hosts-changed",
            &format!("example.com ssh-ed25519 {KEY_A}\n"),
        );
        let result = verify_host_key("example.com", 22, &public_key(KEY_B), Some(&path));
        assert!(matches!(result, Err(russh::keys::Error::KeyChanged { .. })));
        let _ = std::fs::remove_file(path);
    }
}
