//! IME 锚点与指针坐标换算：纯函数，便于无窗口测试。

/// egui 光标矩形（逻辑点）→ winit IME 锚点（物理像素）。
///
/// 候选窗尺寸至少 1px：零尺寸在部分 IME 下会被忽略，导致候选窗跑到屏幕角落。
pub(super) fn ime_cursor_area(
    rect: egui::Rect,
    pixels_per_point: f32,
) -> (
    winit::dpi::PhysicalPosition<i32>,
    winit::dpi::PhysicalSize<u32>,
) {
    let scale = if pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let min = rect.min * scale;
    let size = rect.size() * scale;
    (
        winit::dpi::PhysicalPosition::new(min.x.round() as i32, min.y.round() as i32),
        winit::dpi::PhysicalSize::new(
            size.x.max(1.0).round() as u32,
            size.y.max(1.0).round() as u32,
        ),
    )
}

/// 把 egui 的 IME 锚点应用到窗口；`ime_allowed` 是宿主的缓存状态。
///
/// 独立成纯函数便于无窗口测试：只依赖 [`crate::host::HostWindow`] 抽象。
pub(super) fn apply_ime_to_window(
    window: &dyn crate::host::HostWindow,
    ime_allowed: &mut bool,
    ime: Option<egui::output::IMEOutput>,
    pixels_per_point: f32,
) {
    let allowed = ime.is_some();
    if *ime_allowed != allowed {
        window.set_ime_allowed(allowed);
        *ime_allowed = allowed;
        log::debug!("[IME] set_ime_allowed({allowed})");
    }
    let Some(ime) = ime else {
        return;
    };
    let (position, size) = ime_cursor_area(ime.cursor_rect, pixels_per_point);
    window.set_ime_cursor_area(position, size);
}

/// 指针事件的坐标除数。
///
/// 布局与绘制都以 `pixels_per_point` 为准；指针必须用**同一个**值换算到点空间。
/// winit 的 `CursorMoved.position` 是**物理像素**，egui 用逻辑点：只除一次 ppp。
///
/// 此前这里多除了一次（ppp²），指针坐标比绘制位置小 1.25 倍。真机取证：
/// 客户端逻辑 (24,240) 在 egui 里变成 (19.2,192)，于是窗口下方约 20% 的区域
/// 永远命中不到——混音台与路由栏底部按钮「点了没反应」都是这个原因。
pub(crate) fn input_scale(ppp: f32) -> f32 {
    if ppp > 0.0 { ppp } else { 1.0 }
}
