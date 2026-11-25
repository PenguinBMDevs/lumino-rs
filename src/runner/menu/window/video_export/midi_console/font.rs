use std::cell::OnceCell;

use ab_glyph::FontArc;

thread_local! {
    /// 进程内字体缓存：`load_monospace_font` 每次都读盘 + 解析 TTF（数 MB），
    /// 1080p 逐帧导出时是毫秒级开销；线程固定后线程内缓存即一次解析、长期复用。
    static MONOSPACE_FONT: OnceCell<Option<FontArc>> = const { OnceCell::new() };
}

/// 加载等宽字体（候选表与平台选择由 `lumino-gfx` 单一维护），线程内缓存。
///
/// 首次调用读盘并解析；之后返回 `FontArc` 浅克隆（Arc 计数）。失败同样缓存
/// `None`，避免每帧重复探测不存在的字体文件。
pub(super) fn load_monospace_font() -> Option<FontArc> {
    MONOSPACE_FONT.with(|cell| cell.get_or_init(read_monospace_font).clone())
}

/// 实际读取 + 解析（仅在缓存首次初始化时执行）。
fn read_monospace_font() -> Option<FontArc> {
    lumino_gfx::midiconsole_renderer::load_monospace_font()
}
