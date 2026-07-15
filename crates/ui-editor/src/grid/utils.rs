//! 工具函数

/// 判断琴键是否为黑键（12平均律）
pub fn is_key_dark(key: isize) -> bool {
    let note_in_octave = key.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 获取 MIDI 音符名称（与 C# 项目 PianoRollCalculations.GetNoteName 逻辑一致）
///
/// 音名映射：C, C#, D, D#, E, F, F#, G, G#, A, A#, B
/// 八度计算：midi_note / 12 - 1（MIDI 0 = C-1, 60 = C4）
pub fn note_name(midi_note: u8) -> String {
    const NOTE_NAMES: &[&str] = &[
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (midi_note / 12) as i16 - 1;
    let note_index = (midi_note % 12) as usize;
    format!("{}{}", NOTE_NAMES[note_index], octave)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 note_name 与 C# 项目 PianoRollCalculations.GetNoteName 输出一致
    #[test]
    fn test_note_name_matches_csharp() {
        // 边界值测试
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(127), "G9");

        // 八度边界测试
        assert_eq!(note_name(11), "B-1");
        assert_eq!(note_name(12), "C0");
        assert_eq!(note_name(23), "B0");
        assert_eq!(note_name(24), "C1");

        // 中央 C (MIDI 60)
        assert_eq!(note_name(60), "C4");

        // 黑键测试
        assert_eq!(note_name(1), "C#-1");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(63), "D#4");
        assert_eq!(note_name(66), "F#4");
        assert_eq!(note_name(68), "G#4");
        assert_eq!(note_name(70), "A#4");

        // 白键测试
        assert_eq!(note_name(62), "D4");
        assert_eq!(note_name(64), "E4");
        assert_eq!(note_name(65), "F4");
        assert_eq!(note_name(67), "G4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(71), "B4");
    }

    /// 验证 is_key_dark 与 C# 项目 PianoRollCalculations.IsBlackKey 逻辑一致
    #[test]
    fn test_is_key_dark_matches_csharp() {
        // C 大调音阶中的白键（0, 2, 4, 5, 7, 9, 11）
        assert!(!is_key_dark(0)); // C
        assert!(is_key_dark(1)); // C#
        assert!(!is_key_dark(2)); // D
        assert!(is_key_dark(3)); // D#
        assert!(!is_key_dark(4)); // E
        assert!(!is_key_dark(5)); // F
        assert!(is_key_dark(6)); // F#
        assert!(!is_key_dark(7)); // G
        assert!(is_key_dark(8)); // G#
        assert!(!is_key_dark(9)); // A
        assert!(is_key_dark(10)); // A#
        assert!(!is_key_dark(11)); // B

        // 重复测试（跨八度）
        assert!(!is_key_dark(12)); // C
        assert!(is_key_dark(13)); // C#
        assert!(!is_key_dark(24)); // C
        assert!(is_key_dark(25)); // C#

        // 边界值
        assert!(!is_key_dark(0));
        assert!(!is_key_dark(127 % 12)); // 127 % 12 = 7 -> G (白键)
        assert!(!is_key_dark(127));
    }
}
