//! 字形灰度图 → 占用网格（UI-023）
//!
//! 新路径按 **key 轴覆盖率** 判定（#213 语义：一个 key 的高度中有多少比例落在
//! 字形内部）；legacy 路径保持旧的面积覆盖率，逐字节复现旧形状。

use super::scale::RasterScale;

/// 灰度图 → 占用网格。
///
/// `threshold` 为百分比（0..=100）。非 legacy 时按 pitch 轴覆盖率：
/// 横向（pitch = y）看每个 y 子行是否有墨，纵向（pitch = x）看每个 x 子列。
/// 这样 cell 的屏幕宽度变化不会稀释阈值（面积口径在 9.6:1 格下会把竖笔从
/// 25% 稀释到 2.6%，默认 50% 直接全丢）。
pub(super) fn alpha_to_occupancy(
    buf: &[u8],
    width: u32,
    cols: usize,
    rows: usize,
    scale: RasterScale,
    threshold: f32,
) -> Vec<Vec<bool>> {
    let mut occ = vec![vec![false; cols]; rows];
    if cols == 0 || rows == 0 || width == 0 {
        return occ;
    }
    let threshold = threshold.clamp(0.0, 100.0) / 100.0;
    let sx = scale.ss_x.max(1) as usize;
    let sy = scale.ss_y.max(1) as usize;
    let width = width as usize;
    let ink_at = |x: usize, y: usize| -> bool { buf.get(y * width + x).copied().unwrap_or(0) > 0 };

    for (r, row) in occ.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            if scale.legacy {
                // 旧行为：cell 面积覆盖率
                let mut ink = 0usize;
                for yy in 0..sy {
                    for xx in 0..sx {
                        if ink_at(c * sx + xx, r * sy + yy) {
                            ink += 1;
                        }
                    }
                }
                let total = sx * sy;
                *cell = ink > 0 && (ink as f32 / total as f32) >= threshold;
                continue;
            }

            // 新行为：pitch 轴覆盖率
            let (covered, axis_samples) = if scale.vertical {
                // pitch = x：每个 x 子列只要有墨就算覆盖
                let mut covered = 0usize;
                for xx in 0..sx {
                    let mut any = false;
                    for yy in 0..sy {
                        if ink_at(c * sx + xx, r * sy + yy) {
                            any = true;
                            break;
                        }
                    }
                    if any {
                        covered += 1;
                    }
                }
                (covered, sx)
            } else {
                // pitch = y：每个 y 子行只要有墨就算覆盖
                let mut covered = 0usize;
                for yy in 0..sy {
                    let mut any = false;
                    for xx in 0..sx {
                        if ink_at(c * sx + xx, r * sy + yy) {
                            any = true;
                            break;
                        }
                    }
                    if any {
                        covered += 1;
                    }
                }
                (covered, sy)
            };
            *cell = covered > 0 && (covered as f32 / axis_samples as f32) >= threshold;
        }
    }
    occ
}
