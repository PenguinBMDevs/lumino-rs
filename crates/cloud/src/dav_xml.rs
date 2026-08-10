//! WebDAV PROPFIND 响应 XML 的容错解析
//!
//! 部分 WebDAV 服务器（常见于国产 NAS / 中间代理）会在 PROPFIND 响应中
//! 直接输出 HTML 实体（如 `&nbsp;`），而 XML 标准仅认可 5 个预定义实体
//! （`&amp; &lt; &gt; &quot; &apos;`）与数字字符引用（`&#160;`）。
//! reqwest_dav 内置的 serde_xml_rs 解析对这类脏数据直接报错
//! （`Unexpected entity: nbsp`），导致目录列表整体失败。
//!
//! 本模块在解析前做实体兼容清洗：
//! - XML 预定义实体：原样保留
//! - 数字字符引用（`&#160;` / `&#x1F;`）：原样保留
//! - 常见 HTML 实体：映射为等价的 XML 数字字符引用（如 `&nbsp;` → `&#160;`）
//! - 未知实体：仅将 `&` 转义为 `&amp;`，解析不中断、原文不丢失

use reqwest_dav::types::list_cmd::ListMultiStatus;

/// HTML 命名实体 → XML 数字字符引用
///
/// 仅收录 WebDAV 文件名/展示名场景常见实体；未收录的实体走兜底转义。
/// XML 预定义实体映射到自身（保持语义不变）。
fn html_entity_table(name: &str) -> Option<&'static str> {
    Some(match name {
        // XML 预定义实体（原样保留）
        "amp" => "&amp;",
        "lt" => "&lt;",
        "gt" => "&gt;",
        "quot" => "&quot;",
        "apos" => "&apos;",
        // 常见 HTML 实体 → 等价 XML 数字字符引用
        "nbsp" => "&#160;",
        "copy" => "&#169;",
        "reg" => "&#174;",
        "trade" => "&#8482;",
        "ndash" => "&#8211;",
        "mdash" => "&#8212;",
        "hellip" => "&#8230;",
        "bull" => "&#8226;",
        "middot" => "&#183;",
        "laquo" => "&#171;",
        "raquo" => "&#187;",
        "lsquo" => "&#8216;",
        "rsquo" => "&#8217;",
        "ldquo" => "&#8220;",
        "rdquo" => "&#8221;",
        "deg" => "&#176;",
        "plusmn" => "&#177;",
        "times" => "&#215;",
        "divide" => "&#247;",
        "micro" => "&#181;",
        "para" => "&#182;",
        "sect" => "&#167;",
        "cent" => "&#162;",
        "pound" => "&#163;",
        "yen" => "&#165;",
        "euro" => "&#8364;",
        "frac12" => "&#189;",
        "frac14" => "&#188;",
        "frac34" => "&#190;",
        _ => return None,
    })
}

/// 扫描以 `&` 开头的实体，返回（实体完整字节长度，实体文本含 `&` 与 `;`）
///
/// 仅接受 XML 合法形态：数字字符引用（`&#n;` / `&#xn;`）与 ASCII 命名实体
/// （`[A-Za-z][A-Za-z0-9]*`）。`&` 后不构成合法实体时返回 `None`。
fn scan_entity(s: &str) -> Option<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut i = 1;
    if i < bytes.len() && bytes[i] == b'#' {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'x' || bytes[i] == b'X') {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i == start {
                return None;
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return None;
            }
        }
    } else {
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i].is_ascii_digit()) {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i < bytes.len() && bytes[i] == b';' {
        Some((i + 1, &s[..i + 1]))
    } else {
        None
    }
}

/// 清洗 WebDAV 响应中的脏 XML 实体，返回可被标准 XML 解析器解析的文本
fn sanitize_dav_xml(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        match scan_entity(rest) {
            Some((len, entity)) => {
                let name = &entity[1..entity.len() - 1];
                if name.starts_with('#') {
                    // 数字字符引用（&#160; / &#x1F;）：XML 原生合法，原样保留
                    out.push_str(entity);
                } else {
                    match html_entity_table(name) {
                        Some(replacement) => out.push_str(replacement),
                        None => {
                            // 未知实体：仅转义 `&`，保留字面文本，解析不中断
                            out.push_str("&amp;");
                            out.push_str(name);
                            out.push(';');
                        }
                    }
                }
                rest = &rest[len..];
            }
            None => {
                // 裸 `&`（非实体）：转义防解析失败
                out.push_str("&amp;");
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 解析 PROPFIND 多状态响应（容错：先清洗脏实体再交给 serde_xml_rs）
pub(crate) fn parse_list_multi_status(
    xml: &str,
) -> std::result::Result<ListMultiStatus, serde_xml_rs::Error> {
    serde_xml_rs::from_str(&sanitize_dav_xml(xml))
}

/// 判断响应内容是否为 HTML 页面（而非 WebDAV XML 响应）
///
/// 部分服务器/反向代理对 PROPFIND 请求返回 200 + 登录页或错误页
/// （`<!DOCTYPE html>` / `<html>` 开头），解析必然失败且报错晦涩。
pub(crate) fn is_html_response(text: &str) -> bool {
    let trimmed = text.trim_start();
    let trimmed = trimmed.strip_prefix('\u{feff}').unwrap_or(trimmed);
    let head: String = trimmed
        .chars()
        .take(64)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

/// 生成响应内容预览（用于解析失败时的诊断信息）
///
/// 压缩空白后截取前 200 字符，超长部分以 `…` 省略。
pub(crate) fn response_preview(text: &str) -> String {
    let compact: String = text.split_whitespace().collect();
    let mut chars = compact.chars();
    let head: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_preserves_predefined_entities() {
        assert_eq!(
            sanitize_dav_xml("&amp; &lt; &gt; &quot; &apos;"),
            "&amp; &lt; &gt; &quot; &apos;"
        );
    }

    #[test]
    fn test_sanitize_maps_nbsp_to_numeric() {
        assert_eq!(sanitize_dav_xml("&nbsp;"), "&#160;");
        assert_eq!(sanitize_dav_xml("a&nbsp;b&nbsp;c"), "a&#160;b&#160;c");
    }

    #[test]
    fn test_sanitize_preserves_numeric_refs() {
        assert_eq!(
            sanitize_dav_xml("&#160; &#x1F; &#X00A0;"),
            "&#160; &#x1F; &#X00A0;"
        );
    }

    #[test]
    fn test_sanitize_unknown_entity_escapes_amp() {
        assert_eq!(sanitize_dav_xml("&foo;"), "&amp;foo;");
        assert_eq!(sanitize_dav_xml("&nbspx;"), "&amp;nbspx;");
    }

    #[test]
    fn test_sanitize_bare_amp() {
        assert_eq!(sanitize_dav_xml("a & b"), "a &amp; b");
        assert_eq!(sanitize_dav_xml("&"), "&amp;");
        assert_eq!(sanitize_dav_xml("& ;"), "&amp; ;");
        assert_eq!(sanitize_dav_xml("&#;"), "&amp;#;");
    }

    #[test]
    fn test_sanitize_common_html_entities() {
        assert_eq!(
            sanitize_dav_xml("&copy; &mdash; &euro; &reg;"),
            "&#169; &#8212; &#8364; &#174;"
        );
    }

    #[test]
    fn test_sanitize_escaped_entity_not_double_processed() {
        // `&amp;nbsp;` 是合法 XML（解析为字面 `&nbsp;`），不应二次处理
        assert_eq!(sanitize_dav_xml("&amp;nbsp;"), "&amp;nbsp;");
    }

    #[test]
    fn test_parse_dirty_propfind_nbsp() {
        // 复现线上问题：服务器在 href 中输出 `&nbsp;`（HTML 实体）
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/dav/目录&nbsp;A/file.txt</D:href>
    <D:propstat>
      <D:status>HTTP/1.1 200 OK</D:status>
      <D:prop>
        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
        <D:resourcetype/>
        <D:getcontentlength>1234</D:getcontentlength>
        <D:getcontenttype>application/text</D:getcontenttype>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let parsed = parse_list_multi_status(xml).expect("含 nbsp 的脏 XML 应能解析");
        assert_eq!(parsed.responses.len(), 1);
        // `&nbsp;` 应被映射为 U+00A0（不换行空格），与服务器语义一致
        assert_eq!(parsed.responses[0].href, "/dav/目录\u{a0}A/file.txt");
    }

    #[test]
    fn test_parse_unknown_entity_propfind() {
        // 未知实体不炸解析，文本按字面保留
        let xml = r#"<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/dav/foo&baz;/f.txt</D:href>
    <D:propstat>
      <D:status>HTTP/1.1 200 OK</D:status>
      <D:prop>
        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
        <D:resourcetype/>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let parsed = parse_list_multi_status(xml).expect("含未知实体的脏 XML 应能解析");
        // `&` 被转义后解析还原为字面文本，未知实体原文保留
        assert_eq!(parsed.responses[0].href, "/dav/foo&baz;/f.txt");
    }

    #[test]
    fn test_is_html_response() {
        assert!(is_html_response("<!DOCTYPE html><html><head>"));
        assert!(is_html_response("<!doctype html>\n<html lang=\"zh\">"));
        assert!(is_html_response(
            "<html><head><meta charset=\"utf-8\"></head></html>"
        ));
        // BOM 前缀也应识别
        assert!(is_html_response("\u{feff}<html><body>"));
        // 前导空白不应影响识别
        assert!(is_html_response("  \n  <HTML><head>"));
        // XML 响应不应误判
        assert!(!is_html_response(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus"
        ));
        assert!(!is_html_response("<D:multistatus xmlns:D=\"DAV:\">"));
        // 空响应与任意文本不应误判
        assert!(!is_html_response(""));
        assert!(!is_html_response("hello world"));
    }

    #[test]
    fn test_response_preview() {
        assert_eq!(response_preview("  a   b  c "), "abc");
        let long = "x".repeat(500);
        let preview = response_preview(&long);
        assert_eq!(preview.chars().count(), 201);
        assert!(preview.ends_with('…'));
        let short = response_preview("short");
        assert_eq!(short, "short");
    }
}
