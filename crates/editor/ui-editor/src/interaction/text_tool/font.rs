use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{FontArc, FontVec};

/// 字体缓存：字形解析较重，按家族名缓存 `FontArc`（内部为 `Arc`，克隆廉价），
/// 避免实时预览每帧重复读盘与解析。
static FONT_CACHE: OnceLock<Mutex<HashMap<String, FontArc>>> = OnceLock::new();

/// 加载字体（按家族名查找系统字体缓存；缺失时回退首个可用字体）
pub(super) fn load_font(family: &str) -> Option<FontArc> {
    // 命中缓存直接返回（Arc 克隆廉价）
    if let Some(cached) = FONT_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(family).cloned())
    {
        return Some(cached);
    }
    use lumino_note_core::font_scanner::get_cached_fonts;
    let fonts = get_cached_fonts();
    let target = fonts
        .iter()
        .find(|f| f.name == family)
        .or_else(|| fonts.iter().find(|f| f.name.eq_ignore_ascii_case(family)))
        .or_else(|| {
            fonts
                .iter()
                .find(|f| f.name.to_lowercase().contains(&family.to_lowercase()))
        })
        .or(fonts.first())?;
    let bytes = std::fs::read(&target.path).ok()?;
    let font_vec = FontVec::try_from_vec(bytes).ok()?;
    let font = FontArc::from(font_vec);
    if let Ok(mut m) = FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        m.insert(family.to_string(), font.clone());
    }
    Some(font)
}
