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
use crate::dav_xml::{is_html_response, parse_list_multi_status, response_preview};
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

/// 判断 Content-Type 是否为 HTML 页面
fn content_type_is_html(ct: Option<&str>) -> bool {
    ct.is_some_and(|ct| ct.contains("text/html") || ct.contains("application/xhtml+xml"))
}

/// 服务器返回 HTML 页面时的处理：响应头 + HTML 源码打到 tracing 日志便于诊断，返回明确错误
///
/// 响应头含关键线索：`Server` 头可区分是 WebDAV 守护进程还是通用 Web 服务
/// （如群晖 NAS 的 davd 与 DSM nginx），`WWW-Authenticate` 表明是否需要认证。
/// `port` 为实际连接端口（地址未显式填写时默认为 80），回传便于用户发现漏填端口。
fn html_response_error(headers: &str, source: &str, port: u16) -> CloudError {
    tracing::warn!(
        "WebDAV 服务器返回了 HTML 页面（非 WebDAV 响应），响应头如下:\n{headers}\nHTML 源码如下:\n{source}"
    );
    CloudError::Protocol(format!(
        "服务器返回了 HTML 页面而非 WebDAV 响应：当前连接端口为 {port}（地址未填写端口时默认 80）。若 WebDAV 服务使用非默认端口（如群晖为 5005/5006），请在地址中显式填写端口（如 主机:5005），并确认地址是 WebDAV 服务端点而非网页管理界面"
    ))
}

/// 格式化响应头（用于诊断日志）
fn format_headers(headers: &reqwest::header::HeaderMap) -> String {
    headers
        .iter()
        .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or("<非 UTF-8 值>")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 非成功状态码 → 分级诊断错误（禁用重定向后各状态码含义明确）
fn status_error(status: reqwest::StatusCode) -> CloudError {
    match status.as_u16() {
        301 | 302 | 303 | 307 | 308 => CloudError::Protocol(format!(
            "服务器返回重定向 (HTTP {status})：WebDAV 客户端不跟随重定向，请确认地址是 WebDAV 服务端点（部分服务器对未认证请求会 302 到登录页）"
        )),
        401 => CloudError::Auth("认证失败 (HTTP 401)：请检查用户名与密码".into()),
        403 => CloudError::Protocol(format!(
            "权限不足 (HTTP {status})：请检查账号对目标目录的访问权限"
        )),
        404 => CloudError::Protocol(
            "资源不存在 (HTTP 404)：请确认 WebDAV 地址路径是否正确（如群晖为 http://IP:5005/ 根路径）".into(),
        ),
        code => CloudError::Protocol(format!("PROPFIND 失败: HTTP {status} (code: {code})")),
    }
}

/// 拼接连接 host：地址 + 独立端口字段
///
/// 配置 UI 中地址与端口分栏填写（`address` 只含域名/IP，端口在 `port` 字段）。
/// 地址字符串已显式包含端口时以地址为准，避免冲突。
fn build_host(address: &str, port: Option<u16>) -> String {
    let trimmed = address.trim();
    // 裸 IPv6 字面量（含 ≥2 个冒号、无协议/路径/方括号）→ 补方括号，如 2408:8207::1 → [2408:8207::1]
    let bracketed = if !trimmed.contains("://")
        && !trimmed.contains('/')
        && !trimmed.contains('[')
        && trimmed.matches(':').count() >= 2
    {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    };
    let mut host = if bracketed.contains("://") {
        bracketed
    } else {
        format!("http://{bracketed}")
    };
    if let Ok(mut parsed) = reqwest::Url::parse(&host) {
        if parsed.port().is_none()
            && let Some(port) = port
        {
            let _ = parsed.set_port(Some(port));
        }
        parsed.to_string()
    } else {
        host
    }
}

#[async_trait]
impl CloudClient for WebdavClient {
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()> {
        let password = decrypt(&conn.password_encrypted)?;
        if conn.address.trim().is_empty() {
            return Err(CloudError::Config("WebDAV 地址不能为空".into()));
        }
        // 地址 + 独立端口字段（端口未填时默认 80；地址已含端口时以地址为准）
        let host = build_host(&conn.address, conn.port);

        let agent = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            // WebDAV 协议必须禁用自动重定向：
            // 1) 重定向会触发 reqwest 方法降级（PROPFIND → GET），协议语义被破坏，
            //    服务器（如群晖 NAS）对未认证 PROPFIND 返回 302 登录页，跟随后
            //    拿到的是 DSM 登录页 HTML 而非 WebDAV 响应
            // 2) 标准 WebDAV 客户端均不跟随重定向
            .redirect(reqwest::redirect::Policy::none())
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

        // 使用 list_raw 拿原始响应，绕开 reqwest_dav 内置的严格 XML 解析：
        // 部分服务器会在 PROPFIND 响应中输出 `&nbsp;` 等 HTML 实体，
        // 内置 serde_xml_rs 解析直接报 `Unexpected entity: nbsp`，导致列表整体失败。
        let resp = client
            .list_raw(&target, Depth::Number(1))
            .await
            .map_err(dav_err)?;
        if !resp.status().is_success() {
            return Err(status_error(resp.status()));
        }
        // 先取 Content-Type 与完整响应头（text() 消费响应后无法再读 header），
        // 与内容嗅探共同判断服务器是否返回了 HTML 页面
        let html_by_content_type = content_type_is_html(
            resp.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
        );
        let headers_snapshot = format_headers(resp.headers());
        let text = resp
            .text()
            .await
            .map_err(|e| CloudError::Protocol(format!("读取 PROPFIND 响应失败: {e}")))?;
        // 服务器返回 HTML 页面（登录页/代理错误页）而非 WebDAV 响应：
        // 状态码可能是 200，但内容解析必炸。命中时响应头与 HTML 源码已由
        // html_response_error 打到 tracing 日志，错误信息回传实际连接端口
        if html_by_content_type || is_html_response(&text) {
            let port = reqwest::Url::parse(&self.host)
                .ok()
                .and_then(|u| u.port_or_known_default())
                .unwrap_or(80);
            return Err(html_response_error(&headers_snapshot, &text, port));
        }
        let parsed = parse_list_multi_status(&text).map_err(|e| {
            CloudError::Protocol(format!(
                "解析 PROPFIND 响应失败: {e}（响应预览: {}）",
                response_preview(&text)
            ))
        })?;
        let entities: Vec<ListEntity> = parsed
            .responses
            .into_iter()
            .map(ListEntity::try_from)
            .collect::<std::result::Result<_, _>>()
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

    #[test]
    fn test_content_type_is_html() {
        assert!(content_type_is_html(Some("text/html; charset=utf-8")));
        assert!(content_type_is_html(Some("application/xhtml+xml")));
        assert!(!content_type_is_html(Some("text/xml; charset=utf-8")));
        assert!(!content_type_is_html(Some("application/xml")));
        assert!(!content_type_is_html(Some("text/plain")));
        assert!(!content_type_is_html(None));
    }

    #[test]
    fn test_status_error_diagnosis() {
        // 3xx 重定向：明确提示不跟随（禁用重定向后的正常表现）
        for code in [
            reqwest::StatusCode::MOVED_PERMANENTLY,
            reqwest::StatusCode::FOUND,
            reqwest::StatusCode::SEE_OTHER,
            reqwest::StatusCode::TEMPORARY_REDIRECT,
            reqwest::StatusCode::PERMANENT_REDIRECT,
        ] {
            let e = status_error(code);
            assert!(e.to_string().contains("重定向"), "{code} 应提示重定向");
        }
        // 401 → Auth 错误
        assert!(matches!(
            status_error(reqwest::StatusCode::UNAUTHORIZED),
            CloudError::Auth(_)
        ));
        // 403 → 权限提示
        let e = status_error(reqwest::StatusCode::FORBIDDEN);
        assert!(e.to_string().contains("权限"));
        // 404 → 地址路径提示
        let e = status_error(reqwest::StatusCode::NOT_FOUND);
        assert!(e.to_string().contains("404"));
        // 其他 5xx → 通用提示
        let e = status_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(e.to_string().contains("500"));
    }

    #[test]
    fn test_build_host_with_separate_port() {
        // 独立端口字段：拼接（核心场景：分栏填写的端口必须生效）
        assert_eq!(
            build_host("webdav.example.com", Some(5005)),
            "http://webdav.example.com:5005/"
        );
        // 地址已含显式端口：以地址为准
        assert_eq!(
            build_host("webdav.example.com:5005", Some(5006)),
            "http://webdav.example.com:5005/"
        );
        // 端口字段为 None：走默认端口
        assert_eq!(
            build_host("webdav.example.com", None),
            "http://webdav.example.com/"
        );
        // 地址自带协议
        assert_eq!(
            build_host("http://webdav.example.com", Some(5005)),
            "http://webdav.example.com:5005/"
        );
        // https + 路径 + 端口
        assert_eq!(
            build_host("https://webdav.example.com/dav", Some(5006)),
            "https://webdav.example.com:5006/dav"
        );
        // IPv6 裸字面地址（自动补方括号）
        assert_eq!(
            build_host("2408:8207::1", Some(5005)),
            "http://[2408:8207::1]:5005/"
        );
        // IPv6 已带方括号
        assert_eq!(
            build_host("[2408:8207::1]", Some(5005)),
            "http://[2408:8207::1]:5005/"
        );
    }
}
