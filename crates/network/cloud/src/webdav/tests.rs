use super::error::{content_type_is_html, status_error};
use super::path::{build_host, encode_remote_path, is_self, normalize_href};
use crate::error::CloudError;

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
fn test_normalize_href_percent_decode() {
    // 空格 %20 → 空格（用户报告的显示 bug）
    assert_eq!(normalize_href("/dav/My%20Song.lmpj"), "/dav/My Song.lmpj");
    assert_eq!(
        normalize_href("http://example.com/dav/My%20Song.lmpj"),
        "/dav/My Song.lmpj"
    );
    // UTF-8 中文（%E4%B8%AD 等）
    assert_eq!(
        normalize_href("/dav/%E6%96%87%E4%BB%B6.txt"),
        "/dav/文件.txt"
    );
    // %23（#）解码后为字面 #，不再截断
    assert_eq!(normalize_href("/dav/a%23b.txt"), "/dav/a#b.txt");
    // 字面 + 不误转（path 中 + 就是 +，不是空格）
    assert_eq!(normalize_href("/dav/a+b.txt"), "/dav/a+b.txt");
}

#[test]
fn test_encode_remote_path() {
    // 空格 → %20
    assert_eq!(
        encode_remote_path("/dav/My Song.lmpj"),
        "/dav/My%20Song.lmpj"
    );
    // 中文 → UTF-8 百分号编码
    assert_eq!(
        encode_remote_path("/dav/文件.txt"),
        "/dav/%E6%96%87%E4%BB%B6.txt"
    );
    // # ? 必须编码（否则被当 fragment/query）
    assert_eq!(encode_remote_path("/dav/a#b?c.txt"), "/dav/a%23b%3Fc.txt");
    // 分隔符 / 保留
    assert_eq!(encode_remote_path("/a/b/c"), "/a/b/c");
    // 字面 % 编码为 %25
    assert_eq!(encode_remote_path("/dav/100%.txt"), "/dav/100%25.txt");
    // 根路径
    assert_eq!(encode_remote_path("/"), "/");
}

#[test]
fn test_remote_path_roundtrip() {
    // 解码 ↔ 编码往返：服务器 href → 显示 → 操作路径 一致
    let href = "/dav/%E6%96%87%E4%BB%B6%20%E5%90%8D.txt";
    let decoded = normalize_href(href);
    assert_eq!(decoded, "/dav/文件 名.txt");
    assert_eq!(encode_remote_path(&decoded), href);
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
    // 编码 href 解码后与原始路径比较（空格目录不应误判为自身/非自身）
    assert!(is_self("/dav/My%20Song/", "/dav/My Song"));
    assert!(!is_self("/dav/My%20Song/file.txt", "/dav/My Song"));
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
