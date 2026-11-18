use std::cell::OnceCell;
use std::path::PathBuf;

use ab_glyph::{FontArc, FontVec};

thread_local! {
    /// 进程内字体缓存：`load_monospace_font` 每次都读盘 + 解析 TTF（数百 KB），
    /// 1080p 单帧光这一项就是 ms 级；导出线程固定，线程内缓存即可一次解析反复用。
    static MONOSPACE_FONT: OnceCell<Option<FontArc>> = const { OnceCell::new() };
}

/// 用 `ab_glyph` 加载一个等宽字体（优先 Consolas / DejaVuSansMono），线程内缓存。
///
/// 首次调用读盘解析，之后返回 `FontArc` 浅克隆（Arc 计数）；解析失败同样缓存
/// `None`，避免每帧重复探测不存在的字体文件。
pub(super) fn load_monospace_font() -> Option<FontArc> {
    MONOSPACE_FONT.with(|cell| cell.get_or_init(read_monospace_font).clone())
}

/// 实际读盘 + 解析（仅在缓存首次填充时执行）。
fn read_monospace_font() -> Option<FontArc> {
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
