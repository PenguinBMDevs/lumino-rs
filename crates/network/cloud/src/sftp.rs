//! SFTP 客户端实现（基于 russh + russh-sftp，纯 Rust）
//!
//! 支持密码认证。russh 全程异步，无需阻塞线程包装；
//! 由 `CloudManager` 持有的 tokio Runtime 通过 `block_on` 执行。

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use russh::client::{AuthResult, Config as SshConfig, Handle, Handler, connect};
use russh::keys::ssh_key::PublicKey;
use russh_sftp::client::SftpSession;

use crate::client::{CloudClient, basename, join_remote, normalize_remote, resolve_remote};
use crate::crypto::decrypt;
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry};

/// 服务器主机密钥校验处理器
///
/// 首次连接的服务器密钥直接接受（与常见 FTP/SFTP 客户端一致；
/// 不做指纹持久化校验，简化使用）。
struct SshHandler;

impl Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
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

        let config = Arc::new(SshConfig::default());
        let mut session = connect(config, (address.as_str(), port), SshHandler)
            .await
            .map_err(|e| CloudError::Connect(format!("无法连接 {address}:{port}: {e}")))?;

        match session.authenticate_password(&username, &password).await {
            Ok(AuthResult::Success) => {}
            Ok(_) => return Err(CloudError::Auth("SFTP 认证未通过".into())),
            Err(e) => return Err(CloudError::Auth(format!("SFTP 认证失败: {e}"))),
        }

        // 打开 SFTP 子系统
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| CloudError::Protocol(format!("打开会话通道失败: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| CloudError::Protocol(format!("请求 SFTP 子系统失败: {e}")))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| CloudError::Protocol(format!("SFTP 初始化失败: {e}")))?;

        // 校验根目录可访问
        let base = if root_path.is_empty() {
            "/".to_string()
        } else {
            normalize_remote(&root_path)
        };
        sftp.metadata(&base)
            .await
            .map_err(|e| CloudError::Protocol(format!("进入根目录失败 {base}: {e}")))?;

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
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
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
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let bytes = std::fs::read(local).map_err(CloudError::Io)?;
        let remote_path = resolve_remote(&self.base, remote_path);
        // 必须用 create（CREATE|TRUNCATE|WRITE）而非 write：
        // session::write 只带 WRITE 标志，OpenSSH sftp-server 对 WRITE-only
        // 打开不存在的文件返回 NoSuchFile——上传新文件必然失败。
        use tokio::io::AsyncWriteExt;
        let mut file = sftp
            .create(&remote_path)
            .await
            .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
        // 等待全部写入确认与 CLOSE 完成，确保数据落盘
        file.close()
            .await
            .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let remote_path = resolve_remote(&self.base, remote_path);
        let bytes = sftp
            .read(&remote_path)
            .await
            .map_err(|e| CloudError::Protocol(format!("下载失败 {remote_path}: {e}")))?;
        std::fs::write(local, &bytes).map_err(CloudError::Io)?;
        Ok(())
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, to);
        sftp.rename(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("重命名失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let path = resolve_remote(&self.base, path);
        if is_dir {
            sftp.remove_dir(&path)
                .await
                .map_err(|e| CloudError::Protocol(format!("删除目录失败 {path}: {e}")))?;
        } else {
            sftp.remove_file(&path)
                .await
                .map_err(|e| CloudError::Protocol(format!("删除文件失败 {path}: {e}")))?;
        }
        Ok(())
    }

    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, join_remote(to_dir, &basename(&from)).as_str());
        sftp.rename(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("移动失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn create_dir(&mut self, path: &str) -> Result<()> {
        let sftp = self
            .sftp
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("SFTP 未连接".into()))?;
        let path = resolve_remote(&self.base, path);
        sftp.create_dir(&path)
            .await
            .map_err(|e| CloudError::Protocol(format!("创建目录失败 {path}: {e}")))?;
        Ok(())
    }
}

impl Default for SftpClient {
    fn default() -> Self {
        Self::new()
    }
}
