use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{FontArc, FontVec};

/// 字体缓存：字形解析较重，按家族名缓存 `FontArc`（内部为 `Arc`，克隆廉价），
/// 避免实时预览每帧重复读盘与解析。
static FONT_CACHE: OnceLock<Mutex<HashMap<String, FontArc>>> = OnceLock::new();

/// 加载字体（按家族名查找系统字体缓存；缺失时回退首个可用字体）
pub(super) fn load_font(family: &str) -> Option<FontArc> {
    load_font_variant(family, 400, false)
}

/// 按家族、字重、斜体加载字体；结果按变体缓存。
pub(super) fn load_font_variant(family: &str, weight: u16, italic: bool) -> Option<FontArc> {
    let cache_key = format!("{family}|{weight}|{italic}");
    if let Some(cached) = FONT_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&cache_key).cloned())
    {
        return Some(cached);
    }
    use lumino_note_core::font_scanner::get_cached_fonts;
    let fonts = get_cached_fonts();
    let target = pick_face(fonts, family, weight, italic)?;
    let bytes = std::fs::read(&target.path).ok()?;
    let font_vec = FontVec::try_from_vec(bytes).ok()?;
    let font = FontArc::from(font_vec);
    if let Ok(mut m) = FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        m.insert(cache_key, font.clone());
    }
    Some(font)
}

/// 从字体表中选择最接近的 face：先按 family，再按斜体匹配，最后按字重距离。
fn pick_face<'a>(
    fonts: &'a [lumino_note_core::FontInfo],
    family: &str,
    weight: u16,
    italic: bool,
) -> Option<&'a lumino_note_core::FontInfo> {
    let candidates: Vec<&lumino_note_core::FontInfo> = fonts
        .iter()
        .filter(|f| f.name.eq_ignore_ascii_case(family))
        .collect();
    if candidates.is_empty() {
        return fonts.first();
    }
    candidates
        .into_iter()
        .min_by_key(|f| (u32::from(f.italic != italic), f.weight.abs_diff(weight)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_note_core::FontInfo;
    use std::path::PathBuf;

    fn face(name: &str, weight: u16, italic: bool) -> FontInfo {
        FontInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/{name}-{weight}-{italic}.ttf")),
            weight,
            italic,
        }
    }

    #[test]
    fn test_pick_face_prefers_exact_variant_then_weight() {
        let fonts = vec![
            face("Arial", 400, false),
            face("Arial", 700, false),
            face("Arial", 400, true),
        ];
        assert_eq!(
            pick_face(&fonts, "Arial", 700, false).map(|f| f.weight),
            Some(700)
        );
        assert_eq!(
            pick_face(&fonts, "Arial", 400, true).map(|f| f.italic),
            Some(true)
        );
        assert_eq!(
            pick_face(&fonts, "Arial", 900, false).map(|f| f.weight),
            Some(700),
            "无精确字重时取最近字重"
        );
    }
}
