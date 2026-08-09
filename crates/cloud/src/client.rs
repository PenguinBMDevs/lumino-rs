//! 统一云客户端 trait 与工厂

use std::path::Path;

use async_trait::async_trait;

use crate::error::Result;
use crate::model::{CloudConnection, CloudEntry, CloudProtocol};

/// 云存储客户端统一接口
///
/// 所有方法均为异步：FTP/SFTP 的阻塞 IO 在实现内部用 `spawn_blocking` 包装，
/// WebDAV 直接使用原生异步。调用方（`CloudManager`）持有 tokio Runtime 执行。
#[async_trait]
pub trait CloudClient: Send {
    /// 建立连接并认证
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()>;
    /// 断开连接
    async fn disconnect(&mut self) -> Result<()>;
    /// 列出目录内容
    async fn list_dir(&mut self, path: &str) -> Result<Vec<CloudEntry>>;
    /// 上传本地文件到远程路径
    async fn upload_file(&mut self, local: &Path, remote_path: &str) -> Result<()>;
    /// 下载远程文件到本地路径
    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()>;
    /// 重命名（同目录内）
    async fn rename(&mut self, from: &str, to: &str) -> Result<()>;
    /// 删除文件（is_dir=false）或目录（is_dir=true）
    async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()>;
    /// 移动文件/目录到目标目录（云内部）
    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()>;
    /// 创建目录
    async fn create_dir(&mut self, path: &str) -> Result<()>;
}

/// 按协议创建客户端实例
pub fn create_client(protocol: CloudProtocol) -> Result<Box<dyn CloudClient>> {
    match protocol {
        CloudProtocol::Ftp => Ok(Box::new(crate::ftp::FtpClient::new())),
        CloudProtocol::Sftp => Ok(Box::new(crate::sftp::SftpClient::new())),
        CloudProtocol::Webdav => Ok(Box::new(crate::webdav::WebdavClient::new())),
    }
}

/// 拼接远程路径（处理空路径与根路径，避免双斜杠）
pub(crate) fn join_remote(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else if base == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }
}

/// 规范化远程路径为**绝对路径**（统一根起点）
///
/// 部分 FTP 服务器 LIST 输出带 `./` 前缀的相对名字（如 `./Moyingjun`），
/// 若直接透传会污染后续导航与上传路径（`./Moyingjun/...` 在部分服务器
/// 上导致 "No such file"）。本函数：
/// - 丢弃空段与 `.` 段（`//`、`./`、结尾 `/`）
/// - 处理 `..` 段（弹栈）
/// - 相对路径（不以 `/` 开头）按服务器根（登录后目录）拼接
pub(crate) fn normalize_remote(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            seg => stack.push(seg),
        }
    }
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}

/// FTP 命令参数安全化：含空格时用引号包裹（RFC 959 允许 pathname 带引号）
///
/// suppaftp 的 `Command::Store/Retr/Cwd` 直接拼命令（`STOR {p}`），
/// 文件名含空格会拆断命令（如 `STOR Parallel Universe Shifter.lmpj` 被
/// 服务器解析为两个参数 → 550 No such file）。仅在含空格时加引号，
/// 最大化服务器兼容性。
pub(crate) fn quote_path(path: &str) -> String {
    if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

/// 从远程完整路径提取条目名称
pub(crate) fn basename(path: &str) -> String {
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
    fn test_join_remote() {
        assert_eq!(join_remote("", "a.txt"), "a.txt");
        assert_eq!(join_remote("/", "a.txt"), "/a.txt");
        assert_eq!(join_remote("/dav", "a.txt"), "/dav/a.txt");
        assert_eq!(join_remote("/dav/", "a.txt"), "/dav/a.txt");
        assert_eq!(join_remote("/a/b", "c.txt"), "/a/b/c.txt");
    }

    #[test]
    fn test_basename() {
        assert_eq!(basename("a.txt"), "a.txt");
        assert_eq!(basename("/dav/a.txt"), "a.txt");
        assert_eq!(basename("/dav/dir/"), "dir");
        assert_eq!(basename("http://h/dav/a.txt"), "a.txt");
    }

    #[test]
    fn test_normalize_remote() {
        assert_eq!(normalize_remote(""), "/");
        assert_eq!(normalize_remote("/"), "/");
        assert_eq!(normalize_remote("./Moyingjun"), "/Moyingjun");
        assert_eq!(
            normalize_remote("/./Moyingjun/./Lumino-Archive"),
            "/Moyingjun/Lumino-Archive"
        );
        assert_eq!(
            normalize_remote("./Moyingjun/Lumino-Archive/"),
            "/Moyingjun/Lumino-Archive"
        );
        assert_eq!(
            normalize_remote("//Moyingjun//Lumino-Archive"),
            "/Moyingjun/Lumino-Archive"
        );
        assert_eq!(normalize_remote("/a/./b/../c"), "/a/c");
        assert_eq!(
            normalize_remote("/Moyingjun/Lumino-Archive/Parallel Unit.lmpj"),
            "/Moyingjun/Lumino-Archive/Parallel Unit.lmpj"
        );
    }

    #[test]
    fn test_quote_path() {
        assert_eq!(quote_path("/a/b.txt"), "/a/b.txt");
        assert_eq!(
            quote_path("/a/Parallel Unit.lmpj"),
            "\"/a/Parallel Unit.lmpj\""
        );
        assert_eq!(quote_path("Parallel Unit.lmpj"), "\"Parallel Unit.lmpj\"");
    }

    #[test]
    fn test_create_client_all_protocols() {
        for protocol in [
            CloudProtocol::Ftp,
            CloudProtocol::Sftp,
            CloudProtocol::Webdav,
        ] {
            let client = create_client(protocol).expect("创建客户端应成功");
            let _ = client;
        }
    }
}
