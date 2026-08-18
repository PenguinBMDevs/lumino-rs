//! WebDAV 路径编码与规范化工具
//!
//! 路径规范：本模块内统一使用**相对路径**（如 `/dav/dir/file.txt`），
//! 因为 reqwest_dav 内部以 `host + path` 拼接请求 URL，传完整 URL 会出错。
//! 服务器返回的 href 通过 `normalize_href` 规范化为相对路径。

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

/// 路径段编码集合：保留 RFC 3986 pchar（unreserved + sub-delims + `:` `@`），
/// 编码空格、`#`、`?`、`%`、中文等其余字符（`/` 由调用方分段保留）
pub(crate) const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'#')
    .add(b'%')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^')
    .add(b'[')
    .add(b']');

/// 将服务器返回的 href 规范化为相对路径
///
/// - 完整 URL（含 `scheme://`）→ 提取 path 部分
/// - 相对路径 → 原样返回（去除查询参数与 fragment）
/// - 服务器 href 为 URL 编码形式（RFC 4918，如 `%20`），解码为原始文件名
pub(crate) fn normalize_href(href: &str) -> String {
    let cleaned = href.split(['?', '#']).next().unwrap_or(href);
    let decoded = percent_decode_str(cleaned).decode_utf8_lossy();
    if let Some(pos) = decoded.find("://") {
        let after = &decoded[pos + 3..];
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
        decoded.into_owned()
    }
}

/// 对远程路径做 URL 编码（WebDAV 请求路径必须编码，保留 `/` 分隔符）
///
/// 与 `normalize_href` 的解码互为往返：列表拿到的解码路径可直接用于
/// 上传/下载/重命名等操作，不会二次编码。
pub(crate) fn encode_remote_path(path: &str) -> String {
    path.split('/')
        .map(|seg| utf8_percent_encode(seg, PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// 判断条目是否为目录自身（PROPFIND depth=1 会包含自身条目）
pub(crate) fn is_self(href: &str, path: &str) -> bool {
    let href = normalize_href(href);
    href.trim_end_matches('/') == path.trim_end_matches('/')
}

/// 拼接连接 host：地址 + 独立端口字段
///
/// 配置 UI 中地址与端口分栏填写（`address` 只含域名/IP，端口在 `port` 字段）。
/// 地址字符串已显式包含端口时以地址为准，避免冲突。
pub(crate) fn build_host(address: &str, port: Option<u16>) -> String {
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
    let host = if bracketed.contains("://") {
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
