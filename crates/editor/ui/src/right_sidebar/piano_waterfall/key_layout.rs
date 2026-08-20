//! 钢琴键盘布局（纯几何，与渲染解耦，可单测）
//!
//! 标准 MIDI 键盘样式：
//! - 白键音级：C D E F G A B（0,2,4,5,7,9,11）
//! - 黑键音级：C# D# F# G# A#（1,3,6,8,10）
//! - 黑键位于相邻白键边界，中心对齐到「左侧白键数 × 白键宽度」
//! - 黑键宽度约为白键的 58%（与 Miditrail/瀑布流渲染器一致），高度约为总高的 62%（经典钢琴外观）
//!
//! 绘制顺序约定：调用方应先画全部白键、再画全部黑键，使黑键覆盖在白键之上。

/// 是否为黑键（与 `lumino_gfx::is_black_key` 一致：C#/D#/F#/G#/A#）
#[inline]
pub fn is_black_key(key: isize) -> bool {
    let note = key.rem_euclid(12);
    matches!(note, 1 | 3 | 6 | 8 | 10)
}

/// 单个琴键矩形（像素坐标，y 轴向下）
#[derive(Debug, Clone, Copy)]
pub struct KeyRect {
    /// 左边界（像素）
    pub x: f32,
    /// 上边界（像素）
    pub y: f32,
    /// 宽度（像素）
    pub w: f32,
    /// 高度（像素）
    pub h: f32,
    /// 是否为黑键
    pub is_black: bool,
    /// MIDI 键号（0 起；绘制顺序重排后仍携带原始键号，供着色器索引活跃键颜色）
    pub key: u32,
}

/// 构建键盘布局
///
/// - `width` / `height`：键盘绘制区域像素尺寸
/// - `key_count`：键数（默认 128，可扩展至 256 以支持更大音域）
///
/// 返回所有琴键矩形（白键与黑键混合，按音级顺序排列）。
pub fn build_layout(width: f32, height: f32, key_count: u32) -> Vec<KeyRect> {
    if width <= 0.0 || height <= 0.0 || key_count == 0 {
        return Vec::new();
    }

    // 白键总数（至少 1，避免除零）
    let white_count = (0..key_count)
        .filter(|&k| !is_black_key(k as isize))
        .count()
        .max(1);
    let white_w = width / white_count as f32;

    // 经典钢琴样式尺寸（黑键宽度比与瀑布流/Miditrail 渲染器一致）
    let black_w = white_w * 0.58;
    let black_h = height * 0.62;

    let mut rects = Vec::with_capacity(key_count as usize);
    let mut white_index = 0usize; // 当前已遇到的白键数（用于黑键定位）
    for k in 0..key_count {
        if is_black_key(k as isize) {
            // 黑键中心 = 左侧白键数 × 白键宽度（即相邻白键边界）
            let center_x = white_index as f32 * white_w;
            rects.push(KeyRect {
                x: center_x - black_w / 2.0,
                y: 0.0,
                w: black_w,
                h: black_h,
                is_black: true,
                key: k,
            });
        } else {
            rects.push(KeyRect {
                x: white_index as f32 * white_w,
                y: 0.0,
                w: white_w,
                h: height,
                is_black: false,
                key: k,
            });
            white_index += 1;
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_black_key() {
        assert!(!is_black_key(0)); // C
        assert!(is_black_key(1)); // C#
        assert!(!is_black_key(2)); // D
        assert!(is_black_key(3)); // D#
        assert!(!is_black_key(4)); // E
        assert!(!is_black_key(5)); // F
        assert!(is_black_key(6)); // F#
        assert!(!is_black_key(7)); // G
        assert!(is_black_key(8)); // G#
        assert!(!is_black_key(9)); // A
        assert!(is_black_key(10)); // A#
        assert!(!is_black_key(11)); // B
        assert!(!is_black_key(12)); // C (下一个八度)
        assert!(is_black_key(13)); // C#
    }

    #[test]
    fn test_layout_128_keys() {
        let rects = build_layout(1000.0, 100.0, 128);
        assert_eq!(rects.len(), 128, "128 键应返回 128 个矩形");

        let white = rects.iter().filter(|r| !r.is_black).count();
        let black = rects.iter().filter(|r| r.is_black).count();
        // 128 键 = 10 个完整八度(70 白 50 黑) + 8 键(C..G，5 白 3 黑) = 75 白 / 53 黑
        assert_eq!(white, 75);
        assert_eq!(black, 53);
        assert_eq!(white + black, 128);
    }

    #[test]
    fn test_layout_256_keys() {
        let rects = build_layout(2000.0, 120.0, 256);
        assert_eq!(rects.len(), 256);
        let white = rects.iter().filter(|r| !r.is_black).count();
        let black = rects.iter().filter(|r| r.is_black).count();
        // 256 = 21 个完整八度(147 白 105 黑) + 余下 4 键(C..D#：C/D 白，C#/D# 黑)
        // → 149 白 / 107 黑
        assert_eq!(white, 149);
        assert_eq!(black, 107);
    }

    #[test]
    fn test_black_key_centers_between_whites() {
        // 取白键宽度（与具体键数无关），验证黑键中心落在相邻白键边界
        let rects = build_layout(1000.0, 100.0, 10); // C C# D D# E F F# G G# A
        let white_w = rects
            .iter()
            .find(|r| !r.is_black)
            .expect("10 键布局应至少包含一个白键")
            .w;
        // C#(k=1) 中心应在 1*white_w
        let c_sharp = rects[1];
        assert!(c_sharp.is_black);
        assert!((c_sharp.x + c_sharp.w / 2.0 - 1.0 * white_w).abs() < 1e-3);
        // D#(k=3) 中心应在 2*white_w
        let d_sharp = rects[3];
        assert!(d_sharp.is_black);
        assert!((d_sharp.x + d_sharp.w / 2.0 - 2.0 * white_w).abs() < 1e-3);
        // 白键 C(k=0) 占据 0..white_w
        let c = rects[0];
        assert!(!c.is_black);
        assert!((c.x - 0.0).abs() < 1e-3);
        assert!((c.w - white_w).abs() < 1e-3);
    }

    #[test]
    fn test_empty_on_invalid_input() {
        assert!(build_layout(0.0, 100.0, 128).is_empty());
        assert!(build_layout(100.0, 0.0, 128).is_empty());
        assert!(build_layout(100.0, 100.0, 0).is_empty());
    }
}
