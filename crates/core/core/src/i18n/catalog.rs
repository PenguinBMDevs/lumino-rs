//! 文案目录的静态表（按功能域拆分，逐域可并行维护）
//!
//! 每个子模块导出 `pub fn lookup(zh: &str) -> Option<&'static str>`，
//! 内部是**一个 `match` 表达式**（原文 → 英文）：
//!
//! - `match` 对字符串字面量会被 rustc 编译成跳转/哈希分派，2000+ 条也是 O(1) 级别；
//! - **重复 key 会被 rustc 直接拒绝**（unreachable pattern），不需要人工查重；
//! - 每个文件只写一个函数，多域并行编辑互不冲突。
//!
//! 文件命名按「谁的字面量」划分，与源码目录一一对应，避免同一 key 两处登记：
//!
//! | 目录 | 负责的源码范围 |
//! |---|---|
//! | `ui_view` | `crates/editor/ui/src/view/**`（各对话框正文） |
//! | `ui_shell` | `crates/editor/ui/src/{root,toolbar,titlebar,sidebar,right_sidebar,host,mixer_panel}**` |
//! | `ui_kit` | `crates/editor/{ui-core,ui-editor,ui-settings,editor-state}/**` |
//! | `runner` | `src/**`（应用宿主 / 菜单 / 生命周期 / 服务） |
//! | `cloud_collab` | `crates/network/{cloud,collaboration}/**` |
//! | `export` | `crates/tools/{export,extras}/**` |
//! | `audio` | `crates/audio/**` |
//! | `gfx_dialog` | `crates/editor/{gfx,dialog}/**` |
//! | `project_core` | `crates/core/{core,project,message,diagnostics}/**` |
//!
//! **维护约定**：一条中文原文只登记一次；登记值必须是英文；命名占位符
//! （`{name}`）在两种语言里必须完全一致——由本文件底部的测试强制。

mod audio;
mod cloud_collab;
mod error_display;
mod export;
mod gfx_dialog;
mod help;
mod project_core;
mod runner_host;
mod runner_menu;
mod ui_kit;
mod ui_shell;
mod ui_view;

/// 目录分域查表函数签名。
type Section = fn(&str) -> Option<&'static str>;

/// 全部目录分域（顺序无关；key 唯一性由测试保证）。
static SECTIONS: &[Section] = &[
    ui_view::lookup,
    ui_shell::lookup,
    ui_kit::lookup,
    runner_menu::lookup,
    runner_host::lookup,
    cloud_collab::lookup,
    export::lookup,
    audio::lookup,
    gfx_dialog::lookup,
    project_core::lookup,
    error_display::lookup,
    help::lookup,
];

/// 查英文译文；未登记返回 `None`（调用方回退中文原文）。
pub fn lookup(zh: &str) -> Option<&'static str> {
    SECTIONS.iter().find_map(|section| section(zh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    /// 目录分域源文件（`include_str!` 保证测试与源码同步）。
    const CATALOG_SOURCES: &[(&str, &str)] = &[
        ("ui_view", include_str!("catalog/ui_view.rs")),
        ("ui_shell", include_str!("catalog/ui_shell.rs")),
        ("ui_kit", include_str!("catalog/ui_kit.rs")),
        ("runner_menu", include_str!("catalog/runner_menu.rs")),
        ("runner_host", include_str!("catalog/runner_host.rs")),
        ("cloud_collab", include_str!("catalog/cloud_collab.rs")),
        ("export", include_str!("catalog/export.rs")),
        ("audio", include_str!("catalog/audio.rs")),
        ("gfx_dialog", include_str!("catalog/gfx_dialog.rs")),
        ("project_core", include_str!("catalog/project_core.rs")),
        ("error_display", include_str!("catalog/error_display.rs")),
        ("help", include_str!("catalog/help.rs")),
    ];

    /// 反转义 Rust 字符串字面量内容（仅覆盖本仓会用到的最小集合，含 `\u{XXXX}`）。
    fn unescape(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                // `\u{2026}` 形式：吃掉花括号里的十六进制码点
                Some('u') if chars.peek() == Some(&'{') => {
                    chars.next();
                    let mut hex = String::new();
                    for ch in chars.by_ref() {
                        if ch == '}' {
                            break;
                        }
                        hex.push(ch);
                    }
                    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        Some(decoded) => out.push(decoded),
                        None => {
                            out.push_str("\\u{");
                            out.push_str(&hex);
                            out.push('}');
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }
        out
    }

    /// 提取源码里全部 `"原文" => Some("English"),` 登记项。
    ///
    /// 逐个找 `Some(`：它**前面**最后一个字面量就是原文，**紧跟其后**的是英文。
    /// 这样无论 rustfmt 把 match 臂排成单行还是 `=> { Some(...) }` 块，都能解析。
    fn parse_entries(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(pos) = rest.find("Some(") {
            let head = &rest[..pos];
            let after = &rest[pos + "Some(".len()..];
            if let Some((en, _tail)) = read_literal(after.trim_start())
                && let Some(zh) = last_literal(head)
            {
                out.push((zh, en));
            }
            rest = after;
        }
        out
    }

    /// 取文本里**最后一个**完整字符串字面量。
    fn last_literal(s: &str) -> Option<String> {
        let chars: Vec<char> = s.chars().collect();
        let mut result = None;
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] != '"' {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            let mut raw = String::new();
            let mut closed = false;
            while j < chars.len() {
                match chars[j] {
                    '\\' => {
                        raw.push('\\');
                        if let Some(next) = chars.get(j + 1) {
                            raw.push(*next);
                        }
                        j += 2;
                        continue;
                    }
                    '"' => {
                        closed = true;
                        break;
                    }
                    c => raw.push(c),
                }
                j += 1;
            }
            if closed {
                result = Some(unescape(&raw));
                i = j + 1;
            } else {
                i += 1;
            }
        }
        result
    }

    /// 从字符串开头读一个 `"..."` 字面量，返回（反转义内容, 剩余部分）。
    fn read_literal(s: &str) -> Option<(String, &str)> {
        let mut chars = s.char_indices();
        if chars.next()?.1 != '"' {
            return None;
        }
        let mut raw = String::new();
        while let Some((idx, c)) = chars.next() {
            match c {
                '\\' => {
                    let (_, escaped) = chars.next()?;
                    raw.push('\\');
                    raw.push(escaped);
                }
                '"' => return Some((unescape(&raw), &s[idx + 1..])),
                _ => raw.push(c),
            }
        }
        None
    }

    /// 提取模板里的占位符名字集合（忽略 `{{` / `}}` 转义）。
    fn placeholders(template: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        let bytes: Vec<char> = template.chars().collect();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == '{' {
                if bytes.get(i + 1) == Some(&'{') {
                    i += 2;
                    continue;
                }
                let mut j = i + 1;
                let mut name = String::new();
                while j < bytes.len() && bytes[j] != '}' {
                    name.push(bytes[j]);
                    j += 1;
                }
                if j < bytes.len() && !name.is_empty() {
                    set.insert(name);
                }
                i = j + 1;
                continue;
            }
            i += 1;
        }
        set
    }

    /// 全部登记项（域 → (原文, 英文)）。
    ///
    /// 先丢弃整行注释，避免文件头示例里的 `Some("English")` 被当成真表项。
    fn all_entries() -> Vec<(&'static str, String, String)> {
        let mut out = Vec::new();
        for (section, src) in CATALOG_SOURCES {
            let code = strip_comment_lines(src);
            for (zh, en) in parse_entries(&code) {
                out.push((*section, zh, en));
            }
        }
        out
    }

    /// 工作区根目录（`crates/core/core` 上溯三级）。
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    "target" | ".git" | "node_modules" | ".codegraph" | ".agents"
                ) {
                    continue;
                }
                collect_rs(&path, out);
            } else if name.ends_with(".rs") {
                out.push(path);
            }
        }
    }

    /// 把源码按「注释行整行丢弃」的规则切成可分析文本。
    ///
    /// 只丢整行注释（`//`、`///`、`//!`、`*` 开头），足以屏蔽文档示例里的
    /// `tr!("…")`；行尾注释里的示例不是本测试的防线（仍有其它测试兜底）。
    fn strip_comment_lines(src: &str) -> String {
        src.lines()
            .map(|line| {
                let t = line.trim_start();
                if t.starts_with("//") || t.starts_with('*') {
                    ""
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 从一段源码里抽出全部 `tr!("…")` / `trf!("…")` / `i18n::tr("…")` 的原文 key。
    fn call_site_keys(src: &str, out: &mut Vec<String>) {
        let src = strip_comment_lines(src);
        let chars: Vec<char> = src.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            // `i18n::tr("…")` —— thiserror / 手写 Display 里的函数式取词。
            if chars[i] == 'i' && chars[i..].starts_with(&['i', '1', '8', 'n', ':', ':', 't', 'r'])
            {
                let mut j = i + "i18n::tr".len();
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(&'(') {
                    j += 1;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if let Some((text, _)) = read_chars_literal(&chars, j) {
                        out.push(text);
                    }
                }
                i += "i18n::tr".len();
                continue;
            }
            if chars[i] != '!' {
                i += 1;
                continue;
            }
            // 向前回溯宏名：`tr!` 或 `trf!`
            let (start, ok) = if i >= 2 && chars[i - 1] == 'r' && chars[i - 2] == 't' {
                (i - 2, true)
            } else if i >= 3 && chars[i - 1] == 'f' && chars[i - 2] == 'r' && chars[i - 3] == 't' {
                (i - 3, true)
            } else {
                (i, false)
            };
            if !ok {
                i += 1;
                continue;
            }
            let boundary_ok =
                start == 0 || !(chars[start - 1].is_alphanumeric() || chars[start - 1] == '_');
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if boundary_ok && chars.get(j) == Some(&'(') {
                j += 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if let Some((text, _)) = read_chars_literal(&chars, j) {
                    out.push(text);
                }
            }
            i += 1;
        }
    }

    /// 从字符切片下标 `start`（应指向 `"`）读一个字面量。
    fn read_chars_literal(chars: &[char], start: usize) -> Option<(String, usize)> {
        if chars.get(start) != Some(&'"') {
            return None;
        }
        let mut raw = String::new();
        let mut j = start + 1;
        while j < chars.len() {
            match chars[j] {
                '\\' => {
                    raw.push('\\');
                    if let Some(next) = chars.get(j + 1) {
                        raw.push(*next);
                    }
                    j += 2;
                    continue;
                }
                '"' => return Some((unescape(&raw), j + 1)),
                c => raw.push(c),
            }
            j += 1;
        }
        None
    }

    /// 扫描器自检：确保门禁不会因为「扫描器坏了」而静默放行。
    #[test]
    fn test_call_site_scanner_recognizes_macros() {
        let src = r#"
            //! 文档里的示例不该被统计：tr!("不该命中")
            fn demo(e: &str) -> String {
                let a = tr!("云存储提醒");
                let b = trf!("连接失败：{e}", e = e);
                let c = "字符串里的 tr!(\"也不该命中\")";
                let d = format!("普通 format 不是 tr");
                let _ = c;
                let _ = d;
                let _ = a;
                b
            }
            let e = mytr!("前缀不是宏边界");
        "#;
        let mut keys = Vec::new();
        call_site_keys(src, &mut keys);
        assert_eq!(
            keys,
            vec!["云存储提醒".to_string(), "连接失败：{e}".to_string()]
        );
    }

    #[test]
    fn test_workspace_scan_reaches_source_tree() {
        let root = workspace_root();
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        assert!(
            files.len() > 200,
            "工作区源码扫描异常：只找到 {} 个 .rs 文件（root={}）",
            files.len(),
            root.display()
        );
    }

    #[test]
    fn test_catalog_keys_unique_across_sections() {
        // 跨域重复本身无害（`lookup` 命中第一个），但**同一 key 的译文必须一致**，
        // 否则同一条中文会在不同界面显示成不同的英文。
        let mut seen: BTreeMap<String, (&'static str, String)> = BTreeMap::new();
        for (section, zh, en) in all_entries() {
            if let Some((prev_section, prev_en)) = seen.get(&zh) {
                assert_eq!(
                    prev_en, &en,
                    "同一 key 在不同分域译法不一致：{zh:?}（{prev_section} → {prev_en:?}，{section} → {en:?}）"
                );
                continue;
            }
            seen.insert(zh, (section, en));
        }
    }

    #[test]
    fn test_catalog_english_non_empty() {
        for (section, zh, en) in all_entries() {
            assert!(!zh.trim().is_empty(), "{section} 存在空的中文 key");
            assert!(!en.trim().is_empty(), "{section} 缺少英文译文：{zh:?}");
            assert_ne!(zh, en, "{section} 的英文与中文完全相同：{zh:?}");
        }
    }

    #[test]
    fn test_catalog_placeholders_match() {
        for (section, zh, en) in all_entries() {
            let zh_ph = placeholders(&zh);
            let en_ph = placeholders(&en);
            assert_eq!(zh_ph, en_ph, "{section} 占位符不一致：zh={zh:?} en={en:?}");
        }
    }

    #[test]
    fn test_catalog_lookup_is_consistent_with_table() {
        for (section, zh, en) in all_entries() {
            let got = lookup(&zh);
            assert_eq!(
                got,
                Some(en.as_str()),
                "{section} 的表项未被 lookup 命中：{zh:?}"
            );
        }
    }

    /// **覆盖率门禁**：工作区里每一处 `tr!("…")` / `trf!("…")` 都必须已登记英文。
    ///
    /// 这是「改了文案忘了翻译」的唯一防线——缺一条就红。
    #[test]
    fn test_every_call_site_key_is_translated() {
        let root = workspace_root();
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        assert!(
            files.len() > 200,
            "工作区源码扫描异常：只找到 {} 个 .rs 文件（root={}）",
            files.len(),
            root.display()
        );

        let mut keys = Vec::new();
        for file in &files {
            // 目录自身（含本文件的扫描器样例）不参与「调用点覆盖率」统计。
            if file
                .to_string_lossy()
                .replace('\\', "/")
                .contains("/core/core/src/i18n")
            {
                continue;
            }
            if let Ok(src) = std::fs::read_to_string(file) {
                call_site_keys(&src, &mut keys);
            }
        }

        let translated: BTreeSet<String> = all_entries().into_iter().map(|(_, zh, _)| zh).collect();
        let missing: BTreeSet<String> = keys
            .iter()
            .filter(|k| !translated.contains(*k) && lookup(k).is_none())
            .cloned()
            .collect();
        assert!(
            missing.is_empty(),
            "以下 {} 条 tr!/trf! 文案缺少英文登记：\n{}",
            missing.len(),
            missing
                .iter()
                .map(|k| format!("  - {k:?}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
