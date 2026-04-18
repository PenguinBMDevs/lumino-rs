//! 工具函数

/// 判断琴键是否为黑键（12平均律）
pub fn is_key_dark(key: isize) -> bool {
    let note_in_octave = key % 12;
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
