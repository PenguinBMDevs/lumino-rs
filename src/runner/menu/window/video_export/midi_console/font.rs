use std::path::PathBuf;

use ab_glyph::{FontArc, FontVec};

/// 用 `ab_glyph` 加载一个等宽字体（优先 Consolas / DejaVuSansMono）
pub(super) fn load_monospace_font() -> Option<FontArc> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let dir =
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()) + "\\Fonts\\";
        vec![
            PathBuf::from(dir.clone() + "consola.ttf"),
            PathBuf::from(dir.clone() + "consolab.ttf"),
            PathBuf::from(dir.clone() + "couri.ttf"),
            PathBuf::from(dir + "arial.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Courier New.ttf"),
            PathBuf::from("/Library/Fonts/Courier New.ttf"),
        ]
    };
    for p in &candidates {
        if let Ok(bytes) = std::fs::read(p)
            && let Ok(f) = FontVec::try_from_vec(bytes)
        {
            return Some(FontArc::from(f));
        }
    }
    None
}
