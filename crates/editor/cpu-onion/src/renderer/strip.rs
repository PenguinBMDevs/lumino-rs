//! 水平滚动窄条渲染（保留示例的增量帧契约；Host 当前走整帧重绘）。

use std::sync::Arc;

use lumino_midi_model::MidiDocument;

use super::CpuOnionRenderer;
use crate::{
    band::{BandJob, render_band},
    camera::CanvasCamera,
    compose,
    frame::CpuFrame,
    index::TrackIndex,
    slot::{put_buffer, take_buffer},
};

impl CpuOnionRenderer {
    /// 只渲染 `[x0, x1)` 窄条并合成进当前帧（滚动增量路径）。
    ///
    /// 不整帧清空；调用方负责先 `shift_frame` 并把 `camera.scroll_x` 对齐到
    /// 帧实际滚动值。窄条与全量帧使用同一光栅化，不省填充 / 描边 / 音轨。
    ///
    /// 总览 LOD 不适用窄条复用：密度按屏幕列重采样，必须整帧重画
    /// （[`super::CpuOnionRenderer::render_full_with_density`]）。
    #[allow(clippy::too_many_arguments)]
    pub fn render_strip(
        &mut self,
        document: Arc<MidiDocument>,
        index: Arc<TrackIndex>,
        camera: &CanvasCamera,
        current_track: usize,
        muted: &[bool],
        x0: u32,
        x1: u32,
    ) -> &CpuFrame {
        self.ensure_frame_size();
        self.ensure_output_current();
        self.visible_notes = 0;
        if x0 >= x1 || x1 > self.width || self.height == 0 {
            return &self.output;
        }

        let (key_min, key_max) = camera.visible_key_range();
        let (tick_lo, tick_hi) = camera.tick_range_for_x(x0 as f32, x1 as f32);
        let job = BandJob {
            x0,
            x1,
            camera: *camera,
            tick_lo,
            tick_hi,
            key_min,
            key_max,
            current_track,
            muted: Arc::from(muted),
            style: Arc::clone(&self.style),
            index,
            document,
            simd: self.simd,
            early_coverage: true,
        };

        let mut buffer = take_buffer(&self.strip_slot);
        let stats = render_band(&job, &mut buffer);
        compose::compose_band(
            &mut self.frame,
            self.width as usize,
            self.height as usize,
            x0,
            x1,
            &buffer,
        );
        compose::write_output(
            &self.frame,
            self.width as usize,
            self.height as usize,
            &mut self.output,
            x0,
            x1,
            self.simd,
        );
        put_buffer(&self.strip_slot, buffer);
        self.visible_notes = stats.visible_notes;
        &self.output
    }
}
