//! 文字栅格采样比例（UI-023）
//!
//! 屏幕上一个采样格是 `snap × zoom_x` 宽、`zoom_y` 高，通常不是正方形；
//! `ss_x : ss_y` 必须等于该屏幕宽高比，buffer 像素在屏幕上才是各向同性的。
//! `px_per_key` 是 pitch 轴上 1 个 key 长度对应的 buffer 像素，字号 / 字距 /
//! grow 都按它换算（横向取 `ss_y`、纵向取 `ss_x`）。

/// 旧方格采样的基准倍数（legacy 逐字节行为 + 新路径的 pitch 轴基准）
pub(super) const SS: u32 = 4;
/// 新路径缓冲像素总上限（宽 × 高）；超出时按比例缩小两个轴，保持宽高比
const MAX_RASTER_PIXELS: u64 = 1_000_000;
/// 单个轴的硬上限：防止异常 k / snap 让采样数爆掉
const MAX_SS_AXIS: u32 = 512;
/// 硬上限（cap 后仍超此值则拒绝栅格化，避免巨盒 OOM）
pub(super) const HARD_MAX_PIXELS: u64 = MAX_RASTER_PIXELS * 4;

/// 文字栅格的采样比例。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RasterScale {
    /// 每个采样列在缓冲区里的像素数
    pub ss_x: u32,
    /// 每个采样行在缓冲区里的像素数
    pub ss_y: u32,
    /// 1 个 key 长度对应的缓冲区像素（pitch 轴密度）
    pub px_per_key: f32,
    /// 是否纵向卷帘（决定 pitch 轴是 x 还是 y）
    pub vertical: bool,
    /// true：旧 `Text` 形状的 4×4 方格 + 旧填满 / 面积阈值行为
    pub legacy: bool,
}

impl RasterScale {
    /// 旧形状的方格采样：横向/纵向都是 4×4，逐字节复现修复前行为
    pub fn legacy(vertical: bool) -> Self {
        Self {
            ss_x: SS,
            ss_y: SS,
            px_per_key: SS as f32,
            vertical,
            legacy: true,
        }
    }

    /// 由冻结的 `k`（beats per key）与 snap / ppq 推采样比例。
    ///
    /// 横向 cell ratio = `(snap / ppq) / k`；纵向取倒数。
    /// `cols` / `rows` 用于像素上限：超出时同时缩小两轴，保持宽高比。
    pub fn from_k(vertical: bool, snap: f32, ppq: f32, k: f32, cols: usize, rows: usize) -> Self {
        let snap = snap.max(1.0);
        let ppq = ppq.max(1.0);
        let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
        let beats_per_cell = snap / ppq;
        let ratio = if vertical {
            k / beats_per_cell
        } else {
            beats_per_cell / k
        };
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio.clamp(1e-4, 1e4)
        } else {
            1.0
        };
        let base = SS as f32;
        let (ss_x, ss_y) = if ratio >= 1.0 {
            (
                ((base * ratio).round().max(1.0) as u32).min(MAX_SS_AXIS),
                SS,
            )
        } else {
            (
                SS,
                ((base / ratio).round().max(1.0) as u32).min(MAX_SS_AXIS),
            )
        };
        let mut scale = Self {
            ss_x,
            ss_y,
            px_per_key: if vertical { ss_x } else { ss_y } as f32,
            vertical,
            legacy: false,
        };
        scale.cap_pixels(cols, rows);
        scale
    }

    /// 当前比例下的缓冲区尺寸 `(w, h)`
    pub fn buffer_dims(&self, cols: usize, rows: usize) -> (u32, u32) {
        let cols = u32::try_from(cols).unwrap_or(u32::MAX);
        let rows = u32::try_from(rows).unwrap_or(u32::MAX);
        (
            cols.saturating_mul(self.ss_x.max(1)),
            rows.saturating_mul(self.ss_y.max(1)),
        )
    }

    /// 超出总像素上限时等比缩小两轴；比例尽量保持
    fn cap_pixels(&mut self, cols: usize, rows: usize) {
        let (w, h) = self.buffer_dims(cols, rows);
        let total = u64::from(w) * u64::from(h);
        if total <= MAX_RASTER_PIXELS || total == 0 {
            return;
        }
        let factor = (MAX_RASTER_PIXELS as f64 / total as f64).sqrt();
        self.ss_x = ((f64::from(self.ss_x) * factor).round() as u32).max(1);
        self.ss_y = ((f64::from(self.ss_y) * factor).round() as u32).max(1);
        // 取整后可能仍略超上限：逐像素缩较大轴，保证硬上限。
        loop {
            let (w, h) = self.buffer_dims(cols, rows);
            if u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS {
                break;
            }
            if self.ss_x >= self.ss_y && self.ss_x > 1 {
                self.ss_x -= 1;
            } else if self.ss_y > 1 {
                self.ss_y -= 1;
            } else {
                break;
            }
        }
        self.px_per_key = if self.vertical {
            self.ss_x as f32
        } else {
            self.ss_y as f32
        };
    }
}
