//! 工具函数

/// 判断琴键是否为黑键（12平均律）
pub fn is_key_dark(key: isize) -> bool {
    let note_in_octave = key.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 解析十六进制颜色字符串
pub fn parse_color(hex: &str) -> Option<iced_core::Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(iced_core::Color::from_rgb8(r, g, b))
}

/// 自适应纵向网格线间距
///
/// 根据水平缩放级别自动选择最佳网格密度，不同线类型使用不同最小间距阈值：
/// - 细网格线（32/16/8 分音符，线宽 0.3-0.5px）：最小 4px
/// - 拍线（4 分音符，线宽 0.5-1px）：最小 8px
/// - 小节线（线宽 1-4px）：最小 24px，逐级翻倍（2 小节 → 4 小节 → 8 小节）
pub fn adaptive_grid_gap(zoom_x: f32, ppq: f32) -> f32 {
    let fine_min = 4.0;
    let beat_min = 8.0;
    let bar_min = 24.0;

    if ppq / 8.0 * zoom_x >= fine_min {
        ppq / 8.0
    } else if ppq / 4.0 * zoom_x >= fine_min {
        ppq / 4.0
    } else if ppq / 2.0 * zoom_x >= fine_min {
        ppq / 2.0
    } else if ppq * zoom_x >= beat_min {
        ppq
    } else if ppq * 2.0 * zoom_x >= bar_min {
        ppq * 2.0
    } else if ppq * 4.0 * zoom_x >= bar_min {
        ppq * 4.0
    } else if ppq * 8.0 * zoom_x >= bar_min {
        ppq * 8.0
    } else if ppq * 16.0 * zoom_x >= bar_min {
        ppq * 16.0
    } else {
        ppq * 32.0
    }
}
