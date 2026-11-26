//! 字体文件解析（名称 / 路径 → 字体字节 + 字面索引）。
//!
//! 从 `font_scanner.rs` 拆出（REF-001：单文件 ≤600 行）。
//!
//! 系统字体名解析复用 [`super::get_cached_fonts`] 同一次扫描结果，并额外保存
//! **全部本地化家族名别名**：同一个文件可能同时登记 `Microsoft YaHei` /
//! `微软雅黑` / `Microsoft YaHei UI` 等名称，配置里存的是哪种写法都能命中。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 字体文件引用（路径 + 字面索引 + 家族名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFileRef {
    /// 字体文件路径。
    pub path: PathBuf,
    /// 字体集合（`.ttc`）内的字面索引。
    pub index: u32,
    /// 家族名（字体文件内的第一个家族名，用于诊断）。
    pub family: String,
}

/// 已加载的字体面数据（供 egui 等渲染层安装）。
#[derive(Debug, Clone)]
pub struct FontFaceData {
    /// 家族名。
    pub family: String,
    /// 字体文件路径。
    pub path: PathBuf,
    /// 字体集合内的字面索引。
    pub index: u32,
    /// 字体文件字节。
    pub bytes: Vec<u8>,
}

/// 家族名（含本地化别名，统一小写）→ 字体文件引用。
static FONT_FILES: OnceLock<HashMap<String, FontFileRef>> = OnceLock::new();

/// 从已扫描的字体库建立别名索引（由 [`super::get_cached_fonts`] 调用一次）。
///
/// 必须与字体列表共用同一个 `fontdb::Database`：Windows 上枚举系统字体约 1.3s，
/// 再扫一遍会直接卡住启动。
pub(super) fn build_index(db: &fontdb::Database) {
    // 同一家族可能有多个字面（Regular / Bold / Italic…）：优先常规体，
    // 避免按枚举顺序命中粗体或斜体。
    let mut best: HashMap<String, (u8, FontFileRef)> = HashMap::new();
    for face in db.faces() {
        let Some(path) = source_path(&face.source) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let family = face
            .families
            .first()
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        if family.is_empty() {
            continue;
        }
        let priority =
            if face.style == fontdb::Style::Normal && face.weight == fontdb::Weight::NORMAL {
                0
            } else if face.style == fontdb::Style::Normal {
                1
            } else {
                2
            };
        let entry = FontFileRef {
            path,
            index: face.index,
            family,
        };
        for (alias, _) in &face.families {
            if alias.is_empty() {
                continue;
            }
            let key = alias.to_lowercase();
            let replace = best
                .get(&key)
                .is_none_or(|(existing, _)| priority < *existing);
            if replace {
                best.insert(key, (priority, entry.clone()));
            }
        }
    }
    let files = best
        .into_iter()
        .map(|(alias, (_, file))| (alias, file))
        .collect();
    // 索引只建立一次；重复调用（理论上不会发生）保留第一份。
    let _ = FONT_FILES.set(files);
}

/// 解析「名称或路径」到字体文件引用（路径优先于名称）。
///
/// 名称匹配覆盖字体文件内的**全部家族名别名**且不区分大小写。
pub fn resolve_font_file(name_or_path: &str) -> Option<FontFileRef> {
    let trimmed = name_or_path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = Path::new(trimmed);
    if path.is_file() {
        return read_file_ref(path);
    }

    // 确保别名索引已随系统字体缓存初始化（幂等；初始化后立即返回）。
    let _ = super::get_cached_fonts();
    FONT_FILES.get()?.get(&trimmed.to_lowercase()).cloned()
}

/// 读取「名称或路径」对应的字体字节与字面索引。
///
/// 返回 `None` 表示名称无法解析、文件不可读或字体无法被 `ab_glyph` 解析。
/// 调用方应回退默认字体，不得 panic。
pub fn load_font_face(name_or_path: &str) -> Option<FontFaceData> {
    let file = resolve_font_file(name_or_path)?;
    let bytes = std::fs::read(&file.path).ok()?;
    // 与 epaint 使用同一个解析器先验校验：损坏字体不进渲染层。
    if ab_glyph::FontRef::try_from_slice_and_index(&bytes, file.index).is_err() {
        tracing::warn!(
            "字体文件无法解析，忽略：{:?}（face {}）",
            file.path,
            file.index
        );
        return None;
    }
    Some(FontFaceData {
        family: file.family,
        path: file.path,
        index: file.index,
        bytes,
    })
}

/// 从字体文件读取第一个字面（显式路径场景，调用方无法指定 face）。
fn read_file_ref(path: &Path) -> Option<FontFileRef> {
    let mut db = fontdb::Database::new();
    db.load_font_file(path).ok()?;
    let face = db.faces().next()?;
    Some(FontFileRef {
        path: path.to_path_buf(),
        index: face.index,
        family: face.families.first()?.0.clone(),
    })
}

/// 取字体来源中的实际文件路径（内存字体不参与按路径加载）。
fn source_path(source: &fontdb::Source) -> Option<PathBuf> {
    match source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => Some(path.clone()),
        fontdb::Source::Binary(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_font_file_rejects_empty_and_unknown() {
        assert!(resolve_font_file("").is_none());
        assert!(resolve_font_file("   ").is_none());
        assert!(resolve_font_file("NoSuchFontXYZ").is_none());
    }

    #[test]
    fn test_resolve_font_file_by_display_name_matches_family() {
        let Some(first) = super::super::get_cached_fonts().first() else {
            println!("系统无可用字体，跳过");
            return;
        };
        // 同一家族可能有 Regular / Bold 多个文件（如 AGENCYR / AGENCYB），
        // 索引按常规体优先，路径不保证等于列表里碰巧排在前面的那个字面。
        let resolved = resolve_font_file(&first.name).expect("列表中的家族名必须能解析回文件");
        assert!(resolved.path.is_file(), "解析结果必须是真实字体文件");
        assert!(
            resolved.family.eq_ignore_ascii_case(&first.name),
            "解析出的家族名必须与请求一致: {} != {}",
            resolved.family,
            first.name
        );
    }

    #[test]
    fn test_load_font_face_from_explicit_path() {
        let Some(first) = super::super::get_cached_fonts().first() else {
            println!("系统无可用字体，跳过");
            return;
        };
        let face = load_font_face(&first.path.to_string_lossy()).expect("系统字体文件必须可加载");
        assert!(!face.bytes.is_empty(), "字体字节不能为空");
        assert!(!face.family.is_empty(), "家族名不能为空");
        assert_eq!(face.path, first.path);
    }

    #[test]
    fn test_default_config_font_alias_resolves_when_listed() {
        // 默认配置写的是英文名 `Microsoft YaHei`，部分系统只登记
        // `Microsoft YaHei UI` / `微软雅黑`。别名索引必须让三种写法都能命中。
        const ALIASES: [&str; 3] = ["Microsoft YaHei", "Microsoft YaHei UI", "微软雅黑"];
        let listed = ALIASES.iter().any(|alias| {
            super::super::get_cached_fonts()
                .iter()
                .any(|font| font.name.eq_ignore_ascii_case(alias))
        });
        if !listed {
            println!("系统未登记默认 CJK 字体，跳过");
            return;
        }
        let resolved = ALIASES
            .iter()
            .find_map(|alias| resolve_font_file(alias).map(|file| (*alias, file)))
            .expect("列表中存在默认字体时别名必须可解析");
        println!("默认字体别名 '{}' -> {:?}", resolved.0, resolved.1.path);
        assert!(resolved.1.path.is_file());
    }
}
