//! FTP 客户端实现（基于 suppaftp）
//!
//! FTP 为阻塞 IO，所有操作通过 `tokio::task::spawn_blocking` 包装，
//! 避免阻塞异步运行时线程。

use std::path::Path;

use async_trait::async_trait;
use suppaftp::FtpStream;

use crate::client::{
    CloudClient, basename, join_remote, normalize_remote, quote_path, resolve_remote,
};
use crate::crypto::decrypt;
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry};

/// FTP 客户端
pub struct FtpClient {
    ftp: Option<FtpStream>,
    /// 浏览根基准目录（用户视角 `/` 对应的服务器目录）。
    /// 连接时通过 PWD 获取（root_path 配置则先进入再 PWD），
    /// 所有导航先 CWD 回 base 再进入相对子目录，避免绝对路径
    /// 跳出 root_path / 与服务器 chroot 语义不符。
    base: String,
}

impl FtpClient {
    /// 创建空客户端
    pub fn new() -> Self {
        Self {
            ftp: None,
            base: "/".to_string(),
        }
    }
}

/// 执行阻塞 FTP 操作，返回 (结果, 客户端)
///
/// 闭包内使用 `ftp`，操作完成后原样返回，保证连接不丢失。
fn with_ftp<T>(
    ftp: Option<FtpStream>,
    op: impl FnOnce(&mut FtpStream) -> Result<T> + Send + 'static,
) -> std::thread::JoinHandle<(Result<T>, Option<FtpStream>)>
where
    T: Send + 'static,
{
    std::thread::spawn(move || match ftp {
        Some(mut ftp) => {
            let r = op(&mut ftp);
            (r, Some(ftp))
        }
        None => (Err(CloudError::NotConnected("FTP 未连接".into())), None),
    })
}

#[async_trait]
impl CloudClient for FtpClient {
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()> {
        let password = decrypt(&conn.password_encrypted)?;
        let address = conn.address.clone();
        let port = conn.effective_port();
        let username = conn.username.clone();
        let root_path = conn.root_path.clone();

        let handle = std::thread::spawn(move || -> Result<(FtpStream, String)> {
            let mut ftp = FtpStream::connect((address.as_str(), port))
                .map_err(|e| CloudError::Connect(format!("无法连接 {address}:{port}: {e}")))?;
            ftp.login(&username, &password)
                .map_err(|e| CloudError::Auth(format!("FTP 登录失败: {e}")))?;
            // 进入默认根目录（root_path 为空则保持登录后目录）
            if !root_path.is_empty() {
                ftp.cwd(quote_path(&root_path))
                    .map_err(|e| CloudError::Protocol(format!("进入根目录失败: {e}")))?;
            }
            // PWD 记录浏览根基准（服务器规范化后的真实路径），
            // 后续导航先 CWD 回 base 再进入相对子目录
            let base = ftp
                .pwd()
                .map_err(|e| CloudError::Protocol(format!("获取当前目录失败: {e}")))?;
            Ok((ftp, base))
        });
        let (ftp, base) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 连接线程异常退出".into()))??;
        self.ftp = Some(ftp);
        self.base = base;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        let Some(ftp) = self.ftp.take() else {
            return Ok(());
        };
        let handle = std::thread::spawn(move || -> Result<()> {
            let mut ftp = ftp;
            let _ = ftp.quit();
            Ok(())
        });
        handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 断开线程异常退出".into()))?
    }

    async fn list_dir(&mut self, path: &str) -> Result<Vec<CloudEntry>> {
        let path = path.to_string();
        let base = self.base.clone();
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            // 用户视角路径（/xxx）→ 相对 base 的相对子路径（去前导 /）
            let rel = normalize_remote(&path).trim_start_matches('/').to_string();
            // 先 CWD 回浏览根基准，再进入相对子目录：
            // 绝对路径 CWD 会跳出 root_path / 与服务器 chroot 语义不符
            ftp.cwd(quote_path(&base))
                .map_err(|e| CloudError::Protocol(format!("进入根目录失败 {base}: {e}")))?;
            if !rel.is_empty() {
                ftp.cwd(quote_path(&rel))
                    .map_err(|e| CloudError::Protocol(format!("进入目录失败 {path}: {e}")))?;
            }
            let lines = ftp
                .list(None)
                .map_err(|e| CloudError::Protocol(format!("列出目录失败 {path}: {e}")))?;
            let mut entries = Vec::new();
            for line in &lines {
                if let Some(mut entry) = parse_list_line(line) {
                    // 过滤自身与上级目录标记
                    if matches!(entry.name.as_str(), "." | "..") {
                        continue;
                    }
                    // 条目名可能带 ./ 前缀（部分服务器 LIST 输出），规范化拼接
                    entry.path = normalize_remote(&join_remote(&path, &entry.name));
                    entries.push(entry);
                }
            }
            Ok(entries)
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 列表线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn upload_file(&mut self, local: &Path, remote_path: &str) -> Result<()> {
        let remote_path = normalize_remote(remote_path);
        let local = local.to_path_buf();
        let base = self.base.clone();
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            // 先回到浏览根，再进入目标目录（相对路径）：
            // suppaftp 直接 STOR 完整路径，路径含空格会拆断命令
            // （如 "Parallel Universe Shifter.lmpj"），故分离目录与文件名
            let dir_rel = match remote_path.rfind('/') {
                Some(0) => String::new(),
                Some(idx) => remote_path[..idx].trim_start_matches('/').to_string(),
                None => String::new(),
            };
            let name = basename(&remote_path);
            ftp.cwd(quote_path(&base))
                .map_err(|e| CloudError::Protocol(format!("进入根目录失败 {base}: {e}")))?;
            if !dir_rel.is_empty() {
                ftp.cwd(quote_path(&dir_rel))
                    .map_err(|e| CloudError::Protocol(format!("进入目录失败 {dir_rel}: {e}")))?;
            }
            let mut file = std::fs::File::open(&local).map_err(CloudError::Io)?;
            ftp.put_file(quote_path(&name), &mut file)
                .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 上传线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()> {
        let remote_path = normalize_remote(remote_path);
        let local = local.to_path_buf();
        let base = self.base.clone();
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            // 同上传：先回到浏览根并进入目标目录，再 RETR 文件名
            let dir_rel = match remote_path.rfind('/') {
                Some(0) => String::new(),
                Some(idx) => remote_path[..idx].trim_start_matches('/').to_string(),
                None => String::new(),
            };
            let name = basename(&remote_path);
            ftp.cwd(quote_path(&base))
                .map_err(|e| CloudError::Protocol(format!("进入根目录失败 {base}: {e}")))?;
            if !dir_rel.is_empty() {
                ftp.cwd(quote_path(&dir_rel))
                    .map_err(|e| CloudError::Protocol(format!("进入目录失败 {dir_rel}: {e}")))?;
            }
            let cursor = ftp
                .retr_as_buffer(&quote_path(&name))
                .map_err(|e| CloudError::Protocol(format!("下载失败 {remote_path}: {e}")))?;
            std::fs::write(&local, cursor.into_inner()).map_err(CloudError::Io)?;
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 下载线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        // RNFR/RNTO 使用服务器真实路径（base + 相对段），无需切换目录
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, to);
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            ftp.rename(quote_path(&from), quote_path(&to))
                .map_err(|e| CloudError::Protocol(format!("重命名失败 {from} → {to}: {e}")))?;
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 重命名线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        let path = resolve_remote(&self.base, path);
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            if is_dir {
                ftp.rmdir(quote_path(&path))
                    .map_err(|e| CloudError::Protocol(format!("删除目录失败 {path}: {e}")))?;
            } else {
                ftp.rm(quote_path(&path))
                    .map_err(|e| CloudError::Protocol(format!("删除文件失败 {path}: {e}")))?;
            }
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 删除线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()> {
        let from = resolve_remote(&self.base, from);
        let to = resolve_remote(&self.base, join_remote(to_dir, &basename(&from)).as_str());
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            // FTP RNFR/RNTO 跨目录移动：多数服务器支持（vsftpd/proftpd 均支持）
            ftp.rename(quote_path(&from), quote_path(&to))
                .map_err(|e| CloudError::Protocol(format!("移动失败 {from} → {to}: {e}")))?;
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 移动线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }

    async fn create_dir(&mut self, path: &str) -> Result<()> {
        let path = resolve_remote(&self.base, path);
        let handle = with_ftp(self.ftp.take(), move |ftp| {
            ftp.mkdir(quote_path(&path))
                .map_err(|e| CloudError::Protocol(format!("创建目录失败 {path}: {e}")))?;
            Ok(())
        });
        let (result, ftp) = handle
            .join()
            .map_err(|_| CloudError::Operation("FTP 建目录线程异常退出".into()))?;
        self.ftp = ftp;
        result
    }
}

impl Default for FtpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 FTP `LIST` 单行输出（支持 UNIX 与 Windows/DOS 两种格式）
///
/// UNIX 示例：`-rw-r--r-- 1 user group 1234 Jan 1 12:00 name.txt`
/// 目录：    `drwxr-xr-x 2 user group 4096 Jan 1 12:00 dir`
/// DOS 示例：`01-25-26  12:00PM       <DIR>          foldername`
/// 文件：    `01-25-26  12:00PM             1234 file.txt`
fn parse_list_line(line: &str) -> Option<CloudEntry> {
    let line = line.trim_end_matches('\r');
    if line.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }

    // Windows/DOS 格式：首 token 为日期 MM-DD-YY
    if is_dos_date(tokens[0]) {
        let is_dir = tokens[2].eq_ignore_ascii_case("<DIR>");
        let size = if is_dir {
            0
        } else {
            tokens[2].parse::<u64>().ok()?
        };
        let name = tokens[3..].join(" ");
        return Some(CloudEntry {
            name,
            path: String::new(),
            is_dir,
            size,
            modified: None,
        });
    }

    // UNIX 格式：首 token 为权限位（d/-/l/p）
    let first = *tokens[0].as_bytes().first()?;
    if !matches!(first, b'd' | b'-' | b'l' | b'p') {
        return None;
    }
    let is_dir = first == b'd';
    let size = tokens.get(4)?.parse::<u64>().ok()?;
    let name = tokens.get(8..)?.join(" ");
    if name.is_empty() {
        return None;
    }
    Some(CloudEntry {
        name,
        path: String::new(),
        is_dir,
        size,
        modified: None,
    })
}

/// 判断是否为 DOS 日期格式（MM-DD-YY 或 MM-DD-YYYY）
fn is_dos_date(token: &str) -> bool {
    let b = token.as_bytes();
    b.len() >= 8 && b[2] == b'-' && b[5] == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unix_file_line() {
        let entry = parse_list_line("-rw-r--r-- 1 user group 1234 Jan 1 12:00 song.mid")
            .expect("UNIX 文件行应可解析");
        assert_eq!(entry.name, "song.mid");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 1234);
    }

    #[test]
    fn test_parse_unix_dir_line() {
        let entry = parse_list_line("drwxr-xr-x 2 user group 4096 Jan 1 12:00 my folder")
            .expect("UNIX 目录行应可解析");
        assert_eq!(entry.name, "my folder");
        assert!(entry.is_dir);
    }

    #[test]
    fn test_parse_dos_dir_line() {
        let entry = parse_list_line("01-25-26  12:00PM       <DIR>          folder")
            .expect("DOS 目录行应可解析");
        assert_eq!(entry.name, "folder");
        assert!(entry.is_dir);
        assert_eq!(entry.size, 0);
    }

    #[test]
    fn test_parse_dos_file_line() {
        let entry = parse_list_line("01-25-26  12:00PM             1234 file.txt")
            .expect("DOS 文件行应可解析");
        assert_eq!(entry.name, "file.txt");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 1234);
    }

    #[test]
    fn test_parse_invalid_lines_returns_none() {
        assert!(parse_list_line("").is_none());
        assert!(parse_list_line("220 Welcome to FTP").is_none());
        assert!(parse_list_line("short").is_none());
    }

    #[test]
    fn test_is_dos_date() {
        assert!(is_dos_date("01-25-26"));
        assert!(is_dos_date("01-25-2026"));
        assert!(!is_dos_date("drwxr-xr-x"));
        assert!(!is_dos_date("-rw-r--r--"));
    }
}
