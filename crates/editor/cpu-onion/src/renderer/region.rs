//! 区域重绘（Step 5a）：只重画 `[x, x+w) × [y, y+h)`，其余像素保持不动。
//!
//! # 为什么能保证「区域 = 整帧的裁剪」
//!
//! 光栅化对每个像素的判定只依赖 `(tick, key, 相机) → 屏幕坐标`，且**没有任何
//! 跨像素耦合**（无模糊、无背景、无渐变）。因此把相机滚动平移
//! `(rect.x, rect.y)`、宽高设成区域尺寸后：
//!
//! - 音符的局部坐标 = 全局坐标 − 区域左上角（纯整数平移）；
//! - band 局部 x 从 0 起，`draw_note` 的裁剪窗口变成 `[0, rect.width)`，
//!   纵向裁剪窗口变成 `[0, rect.height)`；
//! - `visible_key_range` / `tick_range_for_x` 对区域与整帧给出的判定对
//!   「与区域相交的音符」完全一致（ceil/floor 的内向取整在本区间内等价）。
//!
//! first-wins（owner 掩码）是逐 band 的：每个像素只属于一个 band，而 band 内
//! 的绘制顺序（轨索引升序 → `start_tick` 升序）在整帧与区域两种切分下完全相同，
//! 故「先画的占住像素」结论也完全相同——band 边界位置不影响结果。
//!
//! 长音符候选来自同一份 [`TrackIndex`]，窗口 lookback 来自同一函数，
//! 因此跨区域的长音符同样只画落在区域内的那段（由 `draw_note` 裁剪）。

use std::sync::Arc;

use lumino_midi_model::MidiDocument;

use super::CpuOnionRenderer;
use crate::{
    band::BandJob, camera::CanvasCamera, compose, frame::CpuFrame, index::TrackIndex,
    slot::put_buffer,
};

/// 帧内像素矩形（单位：像素，左上角 + 尺寸）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuOnionRect {
    /// 左上角 X（含）
    pub x: u32,
    /// 左上角 Y（含）
    pub y: u32,
    /// 宽（> 0 才有意义）
    pub width: u32,
    /// 高（> 0 才有意义）
    pub height: u32,
}

impl CpuOnionRect {
    /// 构造矩形。
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 是否为空矩形（宽或高为 0）。
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 面积（像素数）。
    #[inline]
    #[must_use]
    pub const fn area(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }

    /// 夹到 `[0, max_width] × [0, max_height]`；完全越界返回 `None`。
    ///
    /// 越界部分被**裁剪**而不是平移：区域渲染只应影响它覆盖得到的像素。
    #[must_use]
    pub fn clamp_to(self, max_width: u32, max_height: u32) -> Option<Self> {
        let x0 = self.x.min(max_width);
        let y0 = self.y.min(max_height);
        let x1 = self.x.saturating_add(self.width).min(max_width);
        let y1 = self.y.saturating_add(self.height).min(max_height);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    /// 两个矩形的外接矩形（脏区 union；空矩形不参与扩张）。
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let y1 = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Self {
            x: x0,
            y: y0,
            width: x1.saturating_sub(x0),
            height: y1.saturating_sub(y0),
        }
    }
}

impl CpuOnionRenderer {
    /// 只重画 `rect` 覆盖的区域（先清区域，再重画所有与区域相交的洋葱音符）。
    ///
    /// 区域外的旧像素**原样保留**；区域内的像素与「同参数 `render_full` 后取同
    /// 一块区域」逐像素一致（见 `tests/region_render.rs`）。
    ///
    /// 越界部分被裁剪；完全越界或帧尺寸为 0 时不做任何改动。
    pub fn render_region(
        &mut self,
        document: Arc<MidiDocument>,
        index: Arc<TrackIndex>,
        camera: &CanvasCamera,
        current_track: usize,
        muted: &[bool],
        rect: CpuOnionRect,
    ) -> &CpuFrame {
        self.ensure_frame_size();
        self.visible_notes = 0;
        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 {
            return &self.output;
        }
        let Some(rect) = rect.clamp_to(width, height) else {
            return &self.output;
        };

        // 子相机：区域左上角平移到局部原点（`scroll + rect`）。
        let sub_camera = CanvasCamera {
            width: rect.width,
            height: rect.height,
            scroll_x: camera.scroll_x + rect.x as f32,
            scroll_y: camera.scroll_y + rect.y as f32,
            zoom_x: camera.zoom_x,
            zoom_y: camera.zoom_y,
            key_count: camera.key_count,
        };

        let local_width = rect.width as usize;
        let local_height = rect.height as usize;
        let local_len = local_width.saturating_mul(local_height);
        if self.region_frame.len() != local_len {
            self.region_frame = vec![0; local_len];
        }
        // band 缓冲本身会先清空，这里再填 0 一次保证每个局部像素都被覆盖写入
        // （band 覆盖满局部宽度，任何残留都不可能存活，但显式清零更防未来改动）。
        self.region_frame.fill(0);

        let (key_min, key_max) = sub_camera.visible_key_range();
        let bands = self.band_count.min(rect.width as usize).max(1);
        let band_width = rect.width.div_ceil(bands as u32);
        let muted: Arc<[bool]> = Arc::from(muted);
        let mut ranges = Vec::with_capacity(bands);
        let mut jobs = Vec::with_capacity(bands);
        for band_index in 0..bands {
            let x0 = band_index as u32 * band_width;
            if x0 >= rect.width {
                break;
            }
            let x1 = x0.saturating_add(band_width).min(rect.width);
            let (tick_lo, tick_hi) = sub_camera.tick_range_for_x(x0 as f32, x1 as f32);
            ranges.push((x0, x1));
            jobs.push(BandJob {
                x0,
                x1,
                camera: sub_camera,
                tick_lo,
                tick_hi,
                key_min,
                key_max,
                current_track,
                muted: Arc::clone(&muted),
                style: Arc::clone(&self.style),
                index: Arc::clone(&index),
                document: Arc::clone(&document),
                simd: self.simd,
            });
        }

        let results = self.run_band_jobs(jobs);
        let mut visible = 0u64;
        for (band_index, buffer, stats) in results {
            if let Some(&(x0, x1)) = ranges.get(band_index) {
                compose::compose_band(
                    &mut self.region_frame,
                    local_width,
                    local_height,
                    x0,
                    x1,
                    &buffer,
                );
            }
            visible = visible.saturating_add(stats.visible_notes);
            if let Some(slot) = self.slots.get(band_index) {
                put_buffer(slot, buffer);
            }
        }

        compose::blit_region(
            &mut self.frame,
            width as usize,
            height as usize,
            rect.x,
            rect.y,
            local_width,
            local_height,
            &self.region_frame,
        );
        compose::write_output_region(
            &self.frame,
            width as usize,
            height as usize,
            &mut self.output,
            rect.x,
            rect.x + rect.width,
            rect.y,
            rect.y + rect.height,
        );
        self.visible_notes = visible;
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_crops_out_of_bounds() {
        let rect = CpuOnionRect::new(10, 10, 100, 100);
        assert_eq!(rect.clamp_to(200, 200), Some(rect));
        assert_eq!(
            rect.clamp_to(50, 50),
            Some(CpuOnionRect::new(10, 10, 40, 40)),
            "越界部分必须裁剪，不能平移"
        );
        assert_eq!(rect.clamp_to(10, 10), None, "完全越界必须返回 None");
        assert_eq!(
            CpuOnionRect::new(0, 0, 0, 5).clamp_to(10, 10),
            None,
            "空矩形必须返回 None"
        );
    }

    #[test]
    fn union_covers_both_rects() {
        let a = CpuOnionRect::new(10, 20, 5, 5);
        let b = CpuOnionRect::new(16, 10, 4, 4);
        assert_eq!(a.union(b), CpuOnionRect::new(10, 10, 10, 15));
        let empty = CpuOnionRect::new(3, 3, 0, 0);
        assert_eq!(a.union(empty), a, "空矩形不得扩张并集");
        assert_eq!(empty.union(a), a);
    }

    #[test]
    fn area_and_empty_are_consistent() {
        assert_eq!(CpuOnionRect::new(0, 0, 3, 4).area(), 12);
        assert!(CpuOnionRect::new(0, 0, 0, 4).is_empty());
        assert!(CpuOnionRect::new(0, 0, 4, 0).is_empty());
        assert!(!CpuOnionRect::new(0, 0, 1, 1).is_empty());
    }
}
