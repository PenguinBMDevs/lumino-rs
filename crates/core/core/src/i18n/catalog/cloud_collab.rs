//! 文案目录分域：crates/network/{cloud,collaboration}/**（云存储与多人协作）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // ── crates/network/cloud/src/config.rs ──
        "连接不存在: {id}" => Some("Connection not found: {id}"),
        "序列化配置失败: {e}" => Some("Failed to serialize configuration: {e}"),
        "创建配置目录失败: {e}" => {
            Some("Failed to create the configuration directory: {e}")
        }
        "写入配置失败: {e}" => Some("Failed to write configuration: {e}"),
        "保存配置失败: {e}" => Some("Failed to save configuration: {e}"),
        // ── crates/network/cloud/src/crypto.rs ──
        "密钥长度无效: {e}" => Some("Invalid key length: {e}"),
        "生成随机 nonce 失败: {e}" => Some("Failed to generate a random nonce: {e}"),
        "nonce 长度无效: {e}" => Some("Invalid nonce length: {e}"),
        "加密失败: {e}" => Some("Encryption failed: {e}"),
        "密文 base64 解码失败: {e}" => Some("Failed to decode ciphertext from base64: {e}"),
        "密文长度无效" => Some("Invalid ciphertext length"),
        "解密失败：密文损坏或密钥不匹配" => {
            Some("Decryption failed: corrupted ciphertext or mismatched key")
        }
        "解密结果非 UTF-8: {e}" => Some("Decrypted result is not valid UTF-8: {e}"),
        // ── crates/network/cloud/src/ftp.rs ──
        "FTP 未连接" => Some("FTP not connected"),
        "无法连接 {address}:{port}: {e}" => Some("Cannot connect to {address}:{port}: {e}"),
        "设置 FTP 读超时失败: {e}" => Some("Failed to set the FTP read timeout: {e}"),
        "设置 FTP 写超时失败: {e}" => Some("Failed to set the FTP write timeout: {e}"),
        "FTP 登录失败: {e}" => Some("FTP login failed: {e}"),
        "进入根目录失败: {e}" => Some("Failed to enter the root directory: {e}"),
        "获取当前目录失败: {e}" => Some("Failed to get the current directory: {e}"),
        "FTP 连接线程异常退出" => Some("FTP connection thread exited unexpectedly"),
        "FTP 断开线程异常退出" => Some("FTP disconnect thread exited unexpectedly"),
        "进入根目录失败 {base}: {e}" => {
            Some("Failed to enter the root directory {base}: {e}")
        }
        "进入目录失败 {path}: {e}" => Some("Failed to enter directory {path}: {e}"),
        "列出目录失败 {path}: {e}" => Some("Failed to list directory {path}: {e}"),
        "FTP 列表线程异常退出" => Some("FTP listing thread exited unexpectedly"),
        "进入目录失败 {dir_rel}: {e}" => Some("Failed to enter directory {dir_rel}: {e}"),
        "上传失败 {remote_path}: {e}" => Some("Upload failed {remote_path}: {e}"),
        "FTP 上传线程异常退出" => Some("FTP upload thread exited unexpectedly"),
        "下载失败 {remote_path}: {e}" => Some("Download failed {remote_path}: {e}"),
        "FTP 下载线程异常退出" => Some("FTP download thread exited unexpectedly"),
        "重命名失败 {from} → {to}: {e}" => Some("Rename failed {from} → {to}: {e}"),
        "FTP 重命名线程异常退出" => Some("FTP rename thread exited unexpectedly"),
        "删除目录失败 {path}: {e}" => Some("Failed to delete directory {path}: {e}"),
        "删除文件失败 {path}: {e}" => Some("Failed to delete file {path}: {e}"),
        "FTP 删除线程异常退出" => Some("FTP delete thread exited unexpectedly"),
        "移动失败 {from} → {to}: {e}" => Some("Move failed {from} → {to}: {e}"),
        "FTP 移动线程异常退出" => Some("FTP move thread exited unexpectedly"),
        "创建目录失败 {path}: {e}" => Some("Failed to create directory {path}: {e}"),
        "FTP 建目录线程异常退出" => {
            Some("FTP create-directory thread exited unexpectedly")
        }
        // ── crates/network/cloud/src/manager.rs ──
        "创建全局 tokio 运行时失败: {e}" => {
            Some("Failed to create the global tokio runtime: {e}")
        }
        "云存储未连接: {id}" => Some("Cloud storage not connected: {id}"),
        "读操作重试耗尽" => Some("Read operation retries exhausted"),
        // ── crates/network/cloud/src/sftp.rs ──
        "SFTP 认证未通过" => Some("SFTP authentication rejected"),
        "SFTP 认证失败: {e}" => Some("SFTP authentication failed: {e}"),
        "打开会话通道失败: {e}" => Some("Failed to open a session channel: {e}"),
        "请求 SFTP 子系统失败: {e}" => Some("Failed to request the SFTP subsystem: {e}"),
        "SFTP 初始化失败: {e}" => Some("SFTP initialization failed: {e}"),
        "SFTP 未连接" => Some("SFTP not connected"),
        // ── crates/network/cloud/src/webdav.rs ──
        "WebDAV 地址不能为空" => Some("WebDAV address must not be empty"),
        "创建 HTTP 客户端失败: {e}" => Some("Failed to create the HTTP client: {e}"),
        "WebDAV 未连接" => Some("WebDAV not connected"),
        "读取 PROPFIND 响应失败: {e}" => Some("Failed to read the PROPFIND response: {e}"),
        "解析 PROPFIND 响应失败: {e}（响应预览: {preview}）" => {
            Some("Failed to parse the PROPFIND response: {e} (response preview: {preview})")
        }
        "读取下载数据失败: {e}" => Some("Failed to read the downloaded data: {e}"),
        "删除失败 {path}: {e}" => Some("Delete failed {path}: {e}"),
        // ── crates/network/cloud/src/webdav/error.rs ──
        "服务器返回了 HTML 页面而非 WebDAV 响应：当前连接端口为 {port}（地址未填写端口时默认 80）。若 WebDAV 服务使用非默认端口（如群晖为 5005/5006），请在地址中显式填写端口（如 主机:5005），并确认地址是 WebDAV 服务端点而非网页管理界面" => {
            Some(
                "The server returned an HTML page instead of a WebDAV response: the current connection port is {port} (port 80 is assumed when the address omits it). If the WebDAV service uses a non-default port (5005/5006 on Synology, for example), state the port explicitly in the address (for example host:5005), and make sure the address is the WebDAV service endpoint rather than a web management page",
            )
        }
        "服务器返回重定向 (HTTP {status})：WebDAV 客户端不跟随重定向，请确认地址是 WebDAV 服务端点（部分服务器对未认证请求会 302 到登录页）" => {
            Some(
                "Server returned a redirect (HTTP {status}): the WebDAV client does not follow redirects — make sure the address is the WebDAV service endpoint (some servers answer unauthenticated requests with a 302 to a login page)",
            )
        }
        "认证失败 (HTTP 401)：请检查用户名与密码" => {
            Some("Authentication failed (HTTP 401): check the username and password")
        }
        "权限不足 (HTTP {status})：请检查账号对目标目录的访问权限" => Some(
            "Permission denied (HTTP {status}): check that the account can access the target directory",
        ),
        "资源不存在 (HTTP 404)：请确认 WebDAV 地址路径是否正确（如群晖为 http://IP:5005/ 根路径）" => {
            Some(
                "Resource not found (HTTP 404): make sure the WebDAV address path is correct (on Synology the root path is http://IP:5005/)",
            )
        }
        "PROPFIND 失败: HTTP {status} (code: {code})" => {
            Some("PROPFIND failed: HTTP {status} (code: {code})")
        }
        // ── crates/network/collaboration/src/client/{core,connection,room}.rs ──
        "客户端未连接，当前状态: {state}" => {
            Some("Client not connected, current state: {state}")
        }
        "连接超时（{seconds}秒）" => Some("Connection timed out ({seconds}s)"),
        "意外的认证响应: {text}" => Some("Unexpected authentication response: {text}"),
        "连接已关闭" => Some("Connection closed"),
        "WebSocket 错误: {e}" => Some("WebSocket error: {e}"),
        "认证超时" => Some("Authentication timed out"),
        "意外的消息类型" => Some("Unexpected message type"),
        "请使用 create_room_and_connect" => Some("Please use create_room_and_connect"),
        "请使用 join_room_and_connect" => Some("Please use join_room_and_connect"),
        _ => None,
    }
}
