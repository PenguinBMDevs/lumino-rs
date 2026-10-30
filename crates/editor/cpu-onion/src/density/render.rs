//! 总览热力图的逐像素重采样（逐列 bin / 逐 key 行缓存）。

use super::{DensityPyramid, OverviewScratch, heat_argb};
use crate::camera::CanvasCamera;

impl DensityPyramid {
    /// 把总览热力图写进内部 `0xAARRGGBB` 帧（透明背景）。
    ///
    /// 测试/一次性调用走本入口；生产热路径用
    /// [`Self::render_overview_with_scratch`] 复用缓冲。
    pub fn render_overview(
        &self,
        frame: &mut [u32],
        width: u32,
        height: u32,
        camera: &CanvasCamera,
    ) {
        let mut scratch = OverviewScratch::default();
        self.render_overview_with_scratch(frame, width, height, camera, &mut scratch);
    }

    /// 与 [`Self::render_overview`] 同像素，但复用 `scratch` 的列/行缓冲。
    pub fn render_overview_with_scratch(
        &self,
        frame: &mut [u32],
        width: u32,
        height: u32,
        camera: &CanvasCamera,
        scratch: &mut OverviewScratch,
    ) {
        let width = width as usize;
        let height = height as usize;
        if width == 0
            || height == 0
            || self.key_count == 0
            || frame.len() != width.saturating_mul(height)
        {
            return;
        }

        let level = self.level_for_zoom(camera.zoom_x);
        let Some(level_ref) = self.levels.get(level) else {
            return;
        };
        let bins = level_ref.bins.max(1);
        let bin_ticks = u64::from(level_ref.bin_ticks.max(1));
        let keys = self.key_count as usize;
        let max_count = self.max_count(level);
        let total_ticks = f64::from(self.total_ticks);

        scratch.bin_for_x.clear();
        scratch.bin_for_x.resize(width, usize::MAX);
        for (x, slot) in scratch.bin_for_x.iter_mut().enumerate() {
            let tick = camera.x_to_tick(x as f32 + 0.5);
            if !tick.is_finite() || tick < 0.0 || f64::from(tick) > total_ticks {
                continue;
            }
            let bin = (tick as u64 / bin_ticks) as usize;
            *slot = bin.min(bins - 1);
        }

        // 每个 key 的整行颜色只算一次：overview 缩放下同一 key 覆盖多行，
        // 逐像素算 LUT 会把 128×width 放大成 height×width。
        let row_len = keys.saturating_mul(width);
        scratch.key_rows.clear();
        scratch.key_rows.resize(row_len, 0);
        for key in 0..keys {
            let Some(row) = scratch.key_rows.get_mut(key * width..(key + 1) * width) else {
                continue;
            };
            for (x, pixel) in row.iter_mut().enumerate() {
                let bin = scratch.bin_for_x[x];
                if bin == usize::MAX {
                    continue;
                }
                let count = level_ref
                    .cells
                    .get(bin.saturating_mul(keys).saturating_add(key))
                    .copied()
                    .unwrap_or(0);
                if count > 0 {
                    *pixel = heat_argb(count, max_count);
                }
            }
        }

        let max_key = camera.max_key() as f32;
        for y in 0..height {
            // 行中心 → key 必须用 `ceil`：行 k 覆盖 [key_to_y(k), +zoom_y)，
            // 与网格行边界同口径；`round/floor` 会在行底差一行。
            let key = camera.y_to_key_f32(y as f32 + 0.5).ceil();
            let in_range = key.is_finite() && key >= 0.0 && key <= max_key;
            let key = key as usize;
            let Some(destination) = frame.get_mut(y * width..(y + 1) * width) else {
                continue;
            };
            if !in_range {
                // 视口上下边缘的半行（不在任何 key 内）保持透明。
                destination.fill(0);
                continue;
            }
            let Some(source) = scratch.key_rows.get(key * width..(key + 1) * width) else {
                destination.fill(0);
                continue;
            };
            destination.copy_from_slice(source);
        }
    }
}
