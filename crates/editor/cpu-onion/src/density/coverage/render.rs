//! 彩色覆盖光栅的层选择与逐像素采样（从 `coverage.rs` 拆出）。

use super::{DensityPyramid, EMPTY_TRACK};
use crate::{camera::CanvasCamera, color::NoteStyle};

impl DensityPyramid {
    /// 选择最粗、且 `bin_ticks × zoom_x ≤ 1` 的层；不存在时返回 `None`。
    fn coverage_level_for_zoom(&self, zoom_x: f32) -> Option<usize> {
        if !zoom_x.is_finite() || zoom_x <= 0.0 || self.coverage.is_empty() {
            return None;
        }
        // 允许一个 cell 覆盖到 2×2 屏幕像素：这是 100fps 与可读性的折中。
        // 超过 2px 的块会明显吃掉音符宽度，退回 Exact。
        const MAX_CELL_PX: f32 = 2.0;
        let mut level = 0usize;
        for next in 1..self.coverage.len() {
            if self.coverage[next].bin_ticks as f32 * zoom_x <= MAX_CELL_PX {
                level = next;
            } else {
                break;
            }
        }
        let selected = self.coverage.get(level)?;
        (selected.bin_ticks as f32 * zoom_x <= MAX_CELL_PX).then_some(level)
    }

    /// 彩色覆盖光栅是否可用于当前缩放。
    #[must_use]
    pub fn can_render_coverage(&self, camera: &CanvasCamera) -> bool {
        self.coverage_valid && self.coverage_level_for_zoom(camera.zoom_x).is_some()
    }

    /// 把彩色覆盖光栅写进内部 `0xAARRGGBB` 帧（透明背景）。
    ///
    /// `current_track` / `muted` 参与候选轨道选择：单元第一条被排除时取第二条。
    /// 只有两条都被排除才留空；这个分辨率下第三条与第二条同像素，差异不可见。
    #[allow(clippy::too_many_arguments)]
    pub fn render_coverage_with_scratch(
        &self,
        frame: &mut [u32],
        width: u32,
        height: u32,
        camera: &CanvasCamera,
        style: &NoteStyle,
        current_track: usize,
        muted: &[bool],
        scratch: &mut crate::density::OverviewScratch,
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
        let Some(level) = self.coverage_level_for_zoom(camera.zoom_x) else {
            return;
        };
        let Some(data) = self.coverage.get(level) else {
            return;
        };
        let keys = self.key_count as usize;
        let bins = data.bins.max(1);
        let bin_ticks = u64::from(data.bin_ticks.max(1));
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

        let allowed = |track: u16| -> bool {
            let track = track as usize;
            track != current_track && !muted.get(track).copied().unwrap_or(false)
        };
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
                let cell = data.get(bin, key, keys);
                if cell.is_empty() {
                    continue;
                }
                let second = cell.second();
                let track = if allowed(cell.first()) {
                    cell.first()
                } else if second != EMPTY_TRACK && allowed(second) {
                    second
                } else {
                    continue;
                };
                *pixel = style.fill(track as usize);
            }
        }

        let max_key = camera.max_key() as f32;
        for y in 0..height {
            let key = camera.y_to_key_f32(y as f32 + 0.5).ceil();
            let in_range = key.is_finite() && key >= 0.0 && key <= max_key;
            let key = key as usize;
            let Some(destination) = frame.get_mut(y * width..(y + 1) * width) else {
                continue;
            };
            if !in_range {
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
