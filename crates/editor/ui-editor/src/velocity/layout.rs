//! 力度 / Tempo / Bend 面板的坐标换算与常量。
//!
//! 从已删除的 iced 绘制层迁出：egui 面板与 wgpu 曲线渲染共用同一套映射，
//! 因此保留为纯函数（无 UI 框架类型）。

use lumino_note_core::midi_types::TempoPoint;

/// Tempo BPM 显示下限（固定）。
pub const TEMPO_BPM_MIN: f64 = 20.0;

/// 力度值 → Y 坐标（0..127，随面板高度线性映射）。
pub fn velocity_to_y(velocity: u8, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = crate::velocity::PANEL_PADDING_Y + crate::velocity::RESIZE_HANDLE_HEIGHT;
    let normalized = velocity as f32 / 127.0;
    max_y - normalized * (max_y - min_y)
}

/// Y 坐标 → 力度值（0..127）。
pub fn y_to_velocity(y: f32, bounds_height: f32) -> u8 {
    let max_y = bounds_height;
    let min_y = crate::velocity::PANEL_PADDING_Y + crate::velocity::RESIZE_HANDLE_HEIGHT;
    let clamped_y = y.clamp(min_y, max_y);
    let normalized = (max_y - clamped_y) / (max_y - min_y);
    (normalized * 127.0).round().clamp(0.0, 127.0) as u8
}

/// 将 BPM 值映射到面板 Y 坐标（`max_bpm` = 当前绘制上限）。
pub fn tempo_bpm_to_y(bpm: f64, max_bpm: f64, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = crate::velocity::PANEL_PADDING_Y + crate::velocity::RESIZE_HANDLE_HEIGHT;
    let normalized = ((bpm - TEMPO_BPM_MIN) / (max_bpm - TEMPO_BPM_MIN)) as f32;
    max_y - normalized * (max_y - min_y)
}

/// 生成 BPM 标尺刻度值（等差 9 档）。
pub fn generate_tempo_levels(max_bpm: f64) -> Vec<f64> {
    let count = 9;
    let step = (max_bpm - TEMPO_BPM_MIN) / (count - 1) as f64;
    (0..count)
        .map(|i| TEMPO_BPM_MIN + step * i as f64)
        .collect()
}

/// 将弯音值 (-8192..=+8191) 映射到面板 Y 坐标。
pub fn bend_value_to_y(value: i16, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = crate::velocity::PANEL_PADDING_Y + crate::velocity::RESIZE_HANDLE_HEIGHT;
    let normalized = (value as f32 + 8192.0) / 16383.0;
    max_y - normalized * (max_y - min_y)
}

/// Tempo 控制点屏幕位置（与 [`tempo_bpm_to_y`] 同一口径）。
pub fn tempo_point_screen_pos(
    point: &TempoPoint,
    bounds_height: f32,
    view: &crate::editor_state::ViewState,
    max_bpm: f64,
) -> iced_core::Point {
    let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
    let point_y = tempo_bpm_to_y(point.bpm, max_bpm, bounds_height);
    iced_core::Point::new(point_x, point_y)
}
