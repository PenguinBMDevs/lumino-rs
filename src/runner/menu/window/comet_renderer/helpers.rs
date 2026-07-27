//! Comet 渲染器共享辅助函数

use lumino_core::palette::current_track_color_f32;
use lumino_midi_loader::MidiDocument;

/// 将 BGRA 帧数据填充为纯黑背景
pub(crate) fn fill_bgra_black(frame: &mut [u8]) {
    frame.fill(0);
    for a in frame.iter_mut().skip(3).step_by(4) {
        *a = 255;
    }
}

/// 在 BGRA 帧上绘制一个填充矩形（批量行填充）
pub(crate) fn fill_bgra_rect(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: [u8; 4],
) {
    if w == 0 || h == 0 {
        return;
    }
    let x_end = (x + w).min(frame_width);
    let y_end = (y + h).min(frame_height);
    let row_bytes = frame_width as usize * 4;
    let start_byte = x as usize * 4;
    let pixel_bytes = (x_end - x) as usize * 4;

    for py in y..y_end {
        let row_start = py as usize * row_bytes + start_byte;
        let row_end = row_start + pixel_bytes;
        if row_end > frame.len() {
            break;
        }
        for ch in frame[row_start..row_end].chunks_exact_mut(4) {
            ch[0] = color[0];
            ch[1] = color[1];
            ch[2] = color[2];
            ch[3] = color[3];
        }
    }
}

/// HSV 到 RGB 转换
///
/// h: 0.0~1.0, s: 0.0~1.0, v: 0.0~1.0
pub(crate) fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h % 1.0;
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 1.0 / 6.0 {
        (c, x, 0.0)
    } else if h < 2.0 / 6.0 {
        (x, c, 0.0)
    } else if h < 3.0 / 6.0 {
        (0.0, c, x)
    } else if h < 4.0 / 6.0 {
        (0.0, x, c)
    } else if h < 5.0 / 6.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

/// 根据轨道索引和力度值生成颜色（BGRA 格式，直接用于 BGRA 帧）
///
/// 使用 lumino 当前选中的调色板作为基准色；
/// 力度越高，颜色越亮。
pub(crate) fn note_color(track_idx: usize, velocity: u8) -> [u8; 4] {
    note_color_from_palette(current_track_color_f32(track_idx), velocity)
}

/// 从固定 RGB 颜色生成力度相关的 BGRA 音符颜色
///
/// 便于单元测试，避免依赖全局当前调色板状态。
pub(crate) fn note_color_from_palette(color: [f32; 4], velocity: u8) -> [u8; 4] {
    let vel_factor = (velocity as f32) / 127.0;
    let brightness = 0.4 + vel_factor * 0.6;
    [
        (color[2] * brightness * 255.0).round() as u8, // B
        (color[1] * brightness * 255.0).round() as u8, // G
        (color[0] * brightness * 255.0).round() as u8, // R
        255,
    ]
}

/// 判断 MIDI 键是否为黑键
pub(crate) fn is_black_key(key: usize) -> bool {
    let note_in_octave = key % 12;
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 收集当前 tick 下活跃的音符
pub(crate) fn collect_active_notes(
    document: &MidiDocument,
    tick: u32,
) -> Vec<(usize, usize, u8, u32)> {
    let mut active = Vec::new();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        for n in track_notes {
            if n.start_tick <= tick && n.end_tick > tick {
                active.push((track_idx, n.key as usize, n.velocity, n.start_tick));
            }
        }
    }
    active
}

/// 从文档中提取可见音符列表
pub(crate) fn collect_visible_notes(
    document: &MidiDocument,
    tick_start: u32,
    tick_end: u32,
) -> Vec<(usize, usize, u32, u32, u8, u32)> {
    let mut notes = Vec::new();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        for n in track_notes {
            if n.end_tick > tick_start && n.start_tick < tick_end {
                let length = n.end_tick.saturating_sub(n.start_tick);
                notes.push((
                    track_idx,
                    n.key as usize,
                    n.start_tick,
                    n.end_tick,
                    n.velocity,
                    length,
                ));
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsv_to_rgb_primary_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), [0, 0, 255]);
    }

    #[test]
    fn test_fill_bgra_black() {
        let mut frame = vec![128u8; 100 * 100 * 4];
        fill_bgra_black(&mut frame);
        for i in (0..frame.len()).step_by(4) {
            assert_eq!(frame[i + 3], 255);
            assert_eq!(frame[i], 0);
        }
    }

    #[test]
    fn test_is_black_key() {
        assert!(!is_black_key(0)); // C
        assert!(is_black_key(1)); // C#
        assert!(!is_black_key(2)); // D
        assert!(is_black_key(3)); // D#
        assert!(is_black_key(6)); // F#
    }

    #[test]
    fn test_note_color_brightness() {
        let base = [1.0f32, 0.5, 0.0, 1.0]; // 固定测试颜色，避免依赖全局调色板
        let bright = note_color_from_palette(base, 127);
        let dim = note_color_from_palette(base, 0);
        assert_eq!(bright[3], 255);
        let bright_sum = bright[0] as u32 + bright[1] as u32 + bright[2] as u32;
        let dim_sum = dim[0] as u32 + dim[1] as u32 + dim[2] as u32;
        assert!(bright_sum >= dim_sum);
    }

    #[test]
    fn test_note_color_bgra_order() {
        // 纯红色 RGBA -> BGRA 中 R 分量最高
        let red = [1.0f32, 0.0, 0.0, 1.0];
        let bgra = note_color_from_palette(red, 127);
        assert_eq!(bgra[0], 0); // B
        assert_eq!(bgra[1], 0); // G
        assert!(bgra[2] > 240); // R
    }
}
