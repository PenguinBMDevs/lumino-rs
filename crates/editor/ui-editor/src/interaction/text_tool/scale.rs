//! 文字栅格的**屏幕比例权威**（UI-023）
//!
//! 一个采样格在屏幕上不是正方形，而是「时间格 × key」的长方形：
//!
//! | 卷帘 | 列（advance） | 行（字形高度） |
//! |---|---|---|
//! | 横向 | 时间格：`snap × zoom_x` 像素 | key：`zoom_y` 像素 |
//! | 纵向 | key：`zoom_y` 像素 | 时间格：`snap × zoom_x` 像素 |
//!
//! 旧路径用固定 `LEGACY_SS = 4` 的正方形采样格光栅化、再整体铺到文本框，
//! 于是字形被拉伸 `(snap·zoom_x) : zoom_y` 倍（默认 192 : 20 = 9.6 : 1）。
//! 本模块把比例收敛成显式结构 [`RasterScale`]：预览、确认与重烘焙共用同一份权威。
//!
//! 比例来源是已有的 `k = zoom_y / (ppq × zoom_x)`（beats per key，见 `ShapeModel.k`），
//! 故「时间格 : key」的像素长度比 = `snap / (k · ppq)`；纵向取倒数（轴向对调）。

/// pitch（key）轴的基准子像素数：另一轴按屏幕比例推
const PITCH_SS: f32 = 4.0;

/// 旧方格采样倍数（兼容路径专用：4×4 正方格 + 面积阈值）
pub(super) const LEGACY_SS: u32 = 4;

/// 单次栅格总像素上限（预览每帧重栅格化，必须有上限）
const MAX_PIXELS: f64 = 1_000_000.0;

/// 时间格 : key 的长度比上下限（防御异常 zoom / ppq，正常值约 1e-2..1e2）
const MIN_TIME_PER_KEY: f32 = 1.0 / 1024.0;
const MAX_TIME_PER_KEY: f32 = 1024.0;

/// 采样比例：`ss_x/ss_y` 是「每个采样格在 buffer 里占多少像素」，可为小数。
///
/// `width(cols)` / `height(rows)` 才给出**整数**像素尺寸——先按总宽取整、再把宽度
/// 均分给各格（见 [`Self::x_range`]），这样比例不会因为逐格取整而被舍入误差拉歪。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RasterScale {
    /// 每个采样格横向的 buffer 像素数（可为小数）
    pub ss_x: f32,
    /// 每个采样格纵向的 buffer 像素数（可为小数）
    pub ss_y: f32,
    /// buffer 像素 / 1 个 key 长度（横向 = `ss_y`、纵向 = `ss_x`）
    pub px_per_key: f32,
    /// 是否纵向卷帘（决定 key 轴挂在 X 还是 Y）
    pub vertical: bool,
    /// 是否走旧方格兼容路径（4×4 + 面积阈值 + 填满框拉伸）
    pub legacy: bool,
}

impl RasterScale {
    /// 旧路径：4×4 正方格。用于已落盘的旧 `TextOp`，行为逐字节保持。
    pub fn legacy(vertical: bool) -> Self {
        Self {
            ss_x: LEGACY_SS as f32,
            ss_y: LEGACY_SS as f32,
            px_per_key: LEGACY_SS as f32,
            vertical,
            legacy: true,
        }
    }

    /// 由冻结的 `k`（beats per key）推算采样比例
    ///
    /// `cols` / `rows` 只参与总像素上限（`MAX_PIXELS`）的等比缩小；
    /// 缩小按同一因子缩放两轴，比例误差只来自一次 f32 乘法。
    pub fn from_k(vertical: bool, snap: f32, ppq: f32, k: f32, cols: usize, rows: usize) -> Self {
        let time_per_key = time_cell_over_key(snap, ppq, k);
        let (mut ss_x, mut ss_y) = if vertical {
            // 纵向：列 = key（pitch 基准）、行 = 时间格
            (PITCH_SS, PITCH_SS * time_per_key)
        } else {
            // 横向：列 = 时间格、行 = key（pitch 基准）
            (PITCH_SS * time_per_key, PITCH_SS)
        };
        // 像素上限：两轴同因子缩小，保持比例
        let total = f64::from(cols.max(1) as f32 * ss_x) * f64::from(rows.max(1) as f32 * ss_y);
        if total.is_finite() && total > MAX_PIXELS {
            let shrink = (MAX_PIXELS / total).sqrt() as f32;
            if shrink.is_finite() && shrink > 0.0 {
                ss_x *= shrink;
                ss_y *= shrink;
            }
        }
        let ss_x = if ss_x.is_finite() {
            ss_x.max(1e-3)
        } else {
            1.0
        };
        let ss_y = if ss_y.is_finite() {
            ss_y.max(1e-3)
        } else {
            1.0
        };
        Self {
            ss_x,
            ss_y,
            px_per_key: if vertical { ss_x } else { ss_y },
            vertical,
            legacy: false,
        }
    }

    /// 栅格宽度（像素）：`cols × ss_x` 整体取整
    pub fn width(&self, cols: usize) -> u32 {
        if cols == 0 {
            return 0;
        }
        (cols as f32 * self.ss_x).round().max(1.0) as u32
    }

    /// 栅格高度（像素）：`rows × ss_y` 整体取整
    pub fn height(&self, rows: usize) -> u32 {
        if rows == 0 {
            return 0;
        }
        (rows as f32 * self.ss_y).round().max(1.0) as u32
    }

    /// 第 `c` 列采样格的像素区间 `[x0, x1)`（按总宽均分，避免逐格取整漂移）
    pub fn x_range(&self, c: usize, cols: usize) -> (u32, u32) {
        evenly_split(self.width(cols), c, cols)
    }

    /// 第 `r` 行采样格的像素区间 `[y0, y1)`
    pub fn y_range(&self, r: usize, rows: usize) -> (u32, u32) {
        evenly_split(self.height(rows), r, rows)
    }
}

/// `[0, total)` 均分给 `cells` 份，返回第 `index` 份的 `[lo, hi)`
///
/// 与逐格 `round(index · scale)` 等价但不会累积漂移：边界恒按
/// `round(total · index / cells)` 计算。
fn evenly_split(total: u32, index: usize, cells: usize) -> (u32, u32) {
    if total == 0 || cells == 0 || index >= cells {
        return (0, 0);
    }
    let total = f64::from(total);
    let lo = (total * index as f64 / cells as f64).round() as u32;
    let hi = (total * (index + 1) as f64 / cells as f64).round() as u32;
    (lo.min(total as u32), hi.min(total as u32))
}

/// 1 个时间格的屏幕长度 / 1 个 key 的屏幕长度
///
/// `k = zoom_y / (ppq · zoom_x)` ⇒ `(snap · zoom_x) / zoom_y = snap / (k · ppq)`。
fn time_cell_over_key(snap: f32, ppq: f32, k: f32) -> f32 {
    if !snap.is_finite() || !ppq.is_finite() || !k.is_finite() {
        return 1.0;
    }
    if snap <= 0.0 || ppq <= 0.0 || k <= 0.0 {
        return 1.0;
    }
    let ratio = f64::from(snap) / (f64::from(k) * f64::from(ppq));
    if !ratio.is_finite() || ratio <= 0.0 {
        return 1.0;
    }
    (ratio as f32).clamp(MIN_TIME_PER_KEY, MAX_TIME_PER_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认视图：snap=1920、ppq=480，`k = 20 / (480 × 0.1) = 0.4167`
    fn default_k() -> f32 {
        20.0 / (480.0 * 0.1)
    }

    #[test]
    fn test_default_horizontal_is_38_by_4() {
        let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 2, 15);
        assert_eq!(
            scale.ss_x.round() as u32,
            38,
            "默认横向：时间格应约 38 子像素（屏幕格 192:20 = 9.6:1）"
        );
        assert_eq!(scale.ss_y, 4.0, "pitch 轴基准恒为 4");
        assert_eq!(scale.px_per_key, 4.0, "横向 px_per_key = ss_y");
        // 总像素：2 时间格 × 15 key = 77 × 60（76.8 向上取整）
        assert_eq!((scale.width(2), scale.height(15)), (77, 60));
    }

    #[test]
    fn test_default_vertical_is_4_by_38() {
        let scale = RasterScale::from_k(true, 1920.0, 480.0, default_k(), 15, 2);
        assert_eq!(scale.ss_x, 4.0, "纵向：pitch 轴是列，基准 4");
        assert_eq!(scale.ss_y.round() as u32, 38, "纵向：时间格是行（9.6:1）");
        assert_eq!(scale.px_per_key, 4.0, "纵向 px_per_key = ss_x");
        assert_eq!((scale.width(15), scale.height(2)), (60, 77));
    }

    #[test]
    fn test_ratio_is_preserved_after_pixel_cap() {
        // 大盒子触发 1M 上限：两轴同因子缩小，比例必须保持
        let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 200, 100);
        let total = f64::from(scale.width(200)) * f64::from(scale.height(100));
        assert!(total <= MAX_PIXELS, "总像素必须被 cap 住，实际 {total}");
        let ratio = scale.ss_x / scale.ss_y;
        let expected = 1920.0_f32 / (default_k() * 480.0);
        assert!(
            (ratio - expected).abs() / expected < 0.01,
            "cap 后比例误差需 <1%：{ratio} vs {expected}"
        );
    }

    #[test]
    fn test_legacy_is_square_4x4() {
        let scale = RasterScale::legacy(false);
        assert!(scale.legacy);
        assert_eq!((scale.ss_x, scale.ss_y), (4.0, 4.0));
        assert_eq!((scale.width(8), scale.height(8)), (32, 32));
        let vertical = RasterScale::legacy(true);
        assert_eq!((vertical.ss_x, vertical.ss_y), (4.0, 4.0));
    }

    #[test]
    fn test_cell_ranges_cover_buffer_without_gaps() {
        let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 3, 2);
        let w = scale.width(3);
        let mut cursor = 0;
        for c in 0..3 {
            let (lo, hi) = scale.x_range(c, 3);
            assert_eq!(lo, cursor, "列区间必须首尾相接（无空洞）");
            assert!(hi > lo, "每列至少 1 像素");
            cursor = hi;
        }
        assert_eq!(cursor, w, "最后一列必须落在 buffer 右缘");
    }

    #[test]
    fn test_invalid_inputs_fall_back_to_square() {
        for (snap, ppq, k) in [
            (f32::NAN, 480.0, 0.4),
            (1920.0, 0.0, 0.4),
            (1920.0, 480.0, 0.0),
            (1920.0, 480.0, f32::INFINITY),
        ] {
            let scale = RasterScale::from_k(false, snap, ppq, k, 2, 2);
            assert_eq!(
                (scale.ss_x, scale.ss_y),
                (4.0, 4.0),
                "非法输入（snap={snap}, ppq={ppq}, k={k}）必须退回正方形"
            );
            assert!(!scale.legacy, "比例非法不等于走旧兼容路径");
        }
    }
}
