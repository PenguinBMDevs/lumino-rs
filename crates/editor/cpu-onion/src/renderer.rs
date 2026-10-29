//! 洋葱皮 CPU 渲染器：tokio 多 band 并行 + 水平滚动增量。
//!
//! 设计要点：
//! - 每帧只把 `Arc<MidiDocument>` / `Arc<TrackIndex>` 分给 tokio task，渲染期间
//!   **不复制任何音符数据**；
//! - band 之间互不重叠，结果按 band 索引回填；
//! - 全量渲染前清空帧为全透明；窄条渲染只覆盖 `[x0, x1)`，由调用方先
//!   [`CpuOnionRenderer::shift_frame`]；
//! - 输出始终是透明背景的 sRGB RGBA8 帧，背景 alpha = 0、音符 alpha = 255。
//!
//! # 滚动约定
//!
//! `shift_frame(dx)` 的 `dx` 与相机滚动增量同向：`dx > 0` 表示视口向右滚动
//! （`scroll_x` 增大），画面内容向左移动。小数滚动时调用方必须：
//! 1. `dx_int = (new_scroll - frame_scroll).round()`；
//! 2. `shift_frame(dx_int)`；
//! 3. 用 `scroll_x = frame_scroll + dx_int` 的相机调用 `render_strip`；
//! 4. `frame_scroll += dx_int`。
//!
//! 窄条使用帧实际滚动值（整数），不能直接用小数 `camera.scroll_x`，否则主区域
//! 与窄条会出现亚像素错位。

use std::sync::Arc;

use lumino_midi_model::MidiDocument;
use tokio::task::JoinSet;

use crate::{
    band::{BandJob, BandStats, render_band},
    buffer::BandBuffer,
    camera::CanvasCamera,
    color::NoteStyle,
    compose,
    frame::CpuFrame,
    index::TrackIndex,
    simd::SimdLevel,
    slot::{BandSlot, put_buffer, take_buffer},
};

/// 纯 CPU 洋葱皮渲染器。
pub struct CpuOnionRenderer {
    runtime: Option<tokio::runtime::Runtime>,
    band_count: usize,
    slots: Vec<Arc<BandSlot>>,
    strip_slot: Arc<BandSlot>,
    style: Arc<NoteStyle>,
    /// 内部 `0xAARRGGBB` 合成帧。
    frame: Vec<u32>,
    /// 对外 RGBA8 输出帧。
    output: CpuFrame,
    width: u32,
    height: u32,
    simd: SimdLevel,
    visible_notes: u64,
}

impl CpuOnionRenderer {
    /// 创建渲染器。
    ///
    /// `worker_threads` 至少为 1；`track_count` 用于预建调色板颜色缓存，
    /// 文档轨数超出缓存时查询路径会即时计算，不会漏画。
    /// tokio runtime 构建失败时自动退化为调用线程顺序渲染（不 panic）。
    #[must_use]
    pub fn new(worker_threads: usize, track_count: usize) -> Self {
        let band_count = worker_threads.max(1);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(band_count)
            .enable_all()
            .build()
            .ok();
        let slots = (0..band_count).map(|_| Arc::new(BandSlot::new())).collect();
        Self {
            runtime,
            band_count,
            slots,
            strip_slot: Arc::new(BandSlot::new()),
            style: Arc::new(NoteStyle::new(track_count)),
            frame: Vec::new(),
            output: CpuFrame::default(),
            width: 0,
            height: 0,
            simd: SimdLevel::detect(),
            visible_notes: 0,
        }
    }

    /// 窗口尺寸变化（重建内部帧缓冲）。
    pub fn resize(&mut self, width: u32, height: u32) {
        let expected_pixels = (width as usize).checked_mul(height as usize);
        let expected_bytes = expected_pixels.and_then(|len| len.checked_mul(4));
        if self.width == width
            && self.height == height
            && expected_pixels == Some(self.frame.len())
            && self.output.width == width
            && self.output.height == height
            && expected_bytes == Some(self.output.pixels.len())
        {
            return;
        }
        self.width = width;
        self.height = height;
        self.ensure_frame_size();
        self.clear_frame();
    }

    /// 调色板切换后重建每轨颜色缓存。
    ///
    /// 与 GPU 洋葱皮流在 `palette_idx` 变化时重建实例颜色同口径；不重建
    /// runtime / 帧缓冲，下一次 `render_full` 生效。
    pub fn refresh_palette(&mut self, track_count: usize) {
        self.style = Arc::new(NoteStyle::new(track_count));
    }

    /// 全量渲染一帧：清空为全透明后重画所有可见洋葱皮音符。
    ///
    /// `camera.width` / `height` 应与 [`Self::resize`] 的帧尺寸一致；高度不一致
    /// 时 band 合成会因尺寸校验被跳过（输出保持透明，不 panic）。
    /// `current_track` 越界或 `muted` 长度不足时按「非当前 / 未静音」处理。
    pub fn render_full(
        &mut self,
        document: Arc<MidiDocument>,
        index: Arc<TrackIndex>,
        camera: &CanvasCamera,
        current_track: usize,
        muted: &[bool],
    ) -> &CpuFrame {
        self.ensure_frame_size();
        self.clear_frame();

        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 {
            return &self.output;
        }

        let (key_min, key_max) = camera.visible_key_range();
        let bands = self.band_count.min(width as usize).max(1);
        let band_width = width.div_ceil(bands as u32);
        let muted: Arc<[bool]> = Arc::from(muted);
        let mut ranges = Vec::with_capacity(bands);
        let mut jobs = Vec::with_capacity(bands);
        for band_index in 0..bands {
            let x0 = band_index as u32 * band_width;
            if x0 >= width {
                break;
            }
            let x1 = x0.saturating_add(band_width).min(width);
            let (tick_lo, tick_hi) = camera.tick_range_for_x(x0 as f32, x1 as f32);
            ranges.push((x0, x1));
            jobs.push(BandJob {
                x0,
                x1,
                camera: *camera,
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
        for (band_index, buffer, stats) in results {
            if let Some(&(x0, x1)) = ranges.get(band_index) {
                compose::compose_band(
                    &mut self.frame,
                    self.width as usize,
                    self.height as usize,
                    x0,
                    x1,
                    &buffer,
                );
            }
            self.visible_notes = self.visible_notes.saturating_add(stats.visible_notes);
            if let Some(slot) = self.slots.get(band_index) {
                put_buffer(slot, buffer);
            }
        }
        compose::write_output(
            &self.frame,
            self.width as usize,
            self.height as usize,
            &mut self.output,
            0,
            width,
        );
        &self.output
    }

    /// 按整数像素平移当前帧（相机滚动增量同向：`dx > 0` 内容左移）。
    ///
    /// 内部 u32 帧与对外 RGBA8 帧同步平移，保证 `frame()` 在 shift 后、
    /// 窄条渲染前仍与相机一致。`|dx| >= width` 时整帧清空。
    pub fn shift_frame(&mut self, dx: i32) {
        if dx == 0 {
            return;
        }
        self.ensure_frame_size();
        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 {
            return;
        }
        compose::shift(&mut self.frame, &mut self.output, width, height, dx);
        if dx.unsigned_abs() >= width {
            self.visible_notes = 0;
        }
    }

    /// 只渲染 `[x0, x1)` 窄条并合成进当前帧（滚动增量路径）。
    ///
    /// 不整帧清空；调用方负责先 `shift_frame` 并把 `camera.scroll_x` 对齐到
    /// 帧实际滚动值。窄条与全量帧使用同一光栅化，不省填充 / 描边 / 音轨。
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
        );
        put_buffer(&self.strip_slot, buffer);
        self.visible_notes = stats.visible_notes;
        &self.output
    }

    /// 当前对外帧。
    #[must_use]
    pub fn frame(&self) -> &CpuFrame {
        &self.output
    }

    /// 最近一次 `render_full` / `render_strip` 实际绘制的音符数（诊断用）。
    #[must_use]
    pub fn visible_notes(&self) -> u64 {
        self.visible_notes
    }

    fn ensure_frame_size(&mut self) {
        let width = self.width as usize;
        let height = self.height as usize;
        let len = width.saturating_mul(height);
        if self.frame.len() != len {
            self.frame = vec![0; len];
        }
        let bytes = len.saturating_mul(4);
        if self.output.width != self.width
            || self.output.height != self.height
            || self.output.pixels.len() != bytes
        {
            self.output = CpuFrame::new(self.width, self.height);
        }
    }

    fn clear_frame(&mut self) {
        self.frame.fill(0);
        self.output.clear();
        self.visible_notes = 0;
    }

    fn run_band_jobs(&self, jobs: Vec<BandJob>) -> Vec<(usize, BandBuffer, BandStats)> {
        let Some(runtime) = self.runtime.as_ref() else {
            return self.run_sequential(jobs);
        };
        // 调用方已在 tokio runtime 内时，`block_on` 会 panic；退化为顺序渲染。
        if tokio::runtime::Handle::try_current().is_ok() {
            return self.run_sequential(jobs);
        }
        let mut results = runtime.block_on(async move {
            let mut set = JoinSet::new();
            for (band_index, job) in jobs.into_iter().enumerate() {
                let Some(slot) = self.slots.get(band_index).cloned() else {
                    continue;
                };
                set.spawn(async move {
                    let mut buffer = take_buffer(&slot);
                    let stats = render_band(&job, &mut buffer);
                    (band_index, buffer, stats)
                });
            }
            let mut results = Vec::new();
            while let Some(joined) = set.join_next().await {
                if let Ok(item) = joined {
                    results.push(item);
                }
            }
            results
        });
        results.sort_unstable_by_key(|(band_index, _, _)| *band_index);
        results
    }

    fn run_sequential(&self, jobs: Vec<BandJob>) -> Vec<(usize, BandBuffer, BandStats)> {
        let mut results = Vec::with_capacity(jobs.len());
        for (band_index, job) in jobs.into_iter().enumerate() {
            let Some(slot) = self.slots.get(band_index) else {
                continue;
            };
            let mut buffer = take_buffer(slot);
            let stats = render_band(&job, &mut buffer);
            results.push((band_index, buffer, stats));
        }
        results
    }
}

impl std::fmt::Debug for CpuOnionRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CpuOnionRenderer")
            .field("band_count", &self.band_count)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("simd", &self.simd.name())
            .field("visible_notes", &self.visible_notes)
            .finish()
    }
}

impl Drop for CpuOnionRenderer {
    fn drop(&mut self) {
        // 在 tokio runtime 上下文中直接 drop Runtime 会 panic；后台 shutdown
        // 不等待任务结束，安全且不会阻塞 UI 线程。
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
