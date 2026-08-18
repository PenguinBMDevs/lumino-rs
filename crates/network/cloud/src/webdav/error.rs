//! WebDAV 响应错误分级诊断与诊断日志辅助

use reqwest::StatusCode;
use reqwest::header::HeaderMap;

use crate::error::CloudError;

/// 将 reqwest_dav 错误转换为 CloudError
pub(crate) fn dav_err(e: reqwest_dav::types::Error) -> CloudError {
    CloudError::Protocol(e.to_string())
}

/// 判断 Content-Type 是否为 HTML 页面
pub(crate) fn content_type_is_html(ct: Option<&str>) -> bool {
    ct.is_some_and(|ct| ct.contains("text/html") || ct.contains("application/xhtml+xml"))
}

/// 服务器返回 HTML 页面时的处理：响应头 + HTML 源码打到 tracing 日志便于诊断，返回明确错误
///
/// 响应头含关键线索：`Server` 头可区分是 WebDAV 守护进程还是通用 Web 服务
/// （如群晖 NAS 的 davd 与 DSM nginx），`WWW-Authenticate` 表明是否需要认证。
/// `port` 为实际连接端口（地址未显式填写时默认为 80），回传便于用户发现漏填端口。
pub(crate) fn html_response_error(headers: &str, source: &str, port: u16) -> CloudError {
    tracing::warn!(
        "WebDAV 服务器返回了 HTML 页面（非 WebDAV 响应），响应头如下:\n{headers}\nHTML 源码如下:\n{source}"
    );
    CloudError::Protocol(format!(
        "服务器返回了 HTML 页面而非 WebDAV 响应：当前连接端口为 {port}（地址未填写端口时默认 80）。若 WebDAV 服务使用非默认端口（如群晖为 5005/5006），请在地址中显式填写端口（如 主机:5005），并确认地址是 WebDAV 服务端点而非网页管理界面"
    ))
}

/// 格式化响应头（用于诊断日志）
pub(crate) fn format_headers(headers: &HeaderMap) -> String {
    headers
        .iter()
        .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or("<非 UTF-8 值>")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 非成功状态码 → 分级诊断错误（禁用重定向后各状态码含义明确）
pub(crate) fn status_error(status: StatusCode) -> CloudError {
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
