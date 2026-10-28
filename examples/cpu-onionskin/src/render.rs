//! 渲染层：tokio 并行 band + 直接读取 `MidiDocument`。
//!
//! 整个渲染器不持有任何音符副本：每帧把 `Arc<MidiDocument>` 的引用分给
//! 若干 tokio task，task 内用 `ChunkedList::window_range` + `iter_window`
//! 直接遍历权威数据。编辑层与渲染层共享同一个 Arc。

pub mod band;
pub mod buffer;
pub mod chrome;
pub mod color;
pub mod density;
mod notes;
pub mod simd;

use std::sync::{Arc, Mutex};

use lumino_midi_loader::MidiDocument;
use tokio::task::JoinSet;

use crate::{
    camera::Camera,
    model::TrackIndex,
    render::{
        band::{BandJob, BandStats, LodMode, render_band},
        buffer::BandBuffer,
        color::NoteStyle,
        simd::SimdLevel,
    },
};

/// 一个可复用的 band 槽位。task 期间 buffer 被 take 走，task 结束后归还；
/// 槽位锁只在 take/put 时短暂持有，不跨越 async 执行体。
struct BandSlot {
    buffer: Mutex<Option<BandBuffer>>,
}

impl BandSlot {
    fn new() -> Self {
        Self {
            buffer: Mutex::new(Some(BandBuffer::default())),
        }
    }
}

fn take_buffer(slot: &BandSlot) -> BandBuffer {
    slot.buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .unwrap_or_default()
}

fn put_buffer(slot: &BandSlot, buffer: BandBuffer) {
    *slot
        .buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(buffer);
}

/// 一帧的并行渲染统计。
#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    /// 通过可见过滤并尝试绘制的音符总数。
    pub visible_notes: u64,
    /// 窗口迭代候选总数。
    pub candidates: u64,
    /// 长音符 underlay 扫描总数。
    pub long_candidates: u64,
    /// 实际使用的 band 数。
    pub bands: usize,
    /// 每 band 统计。
    pub per_band: Vec<BandStats>,
}

/// tokio 多 worker + 横向 band 的纯 CPU 渲染器。
pub struct Renderer {
    runtime: tokio::runtime::Runtime,
    band_count: usize,
    slots: Vec<Arc<BandSlot>>,
    strip_slot: Arc<BandSlot>,
    frame: Vec<u32>,
    width: u32,
    height: u32,
    simd: SimdLevel,
    style: Arc<NoteStyle>,
}

impl Renderer {
    /// 创建渲染器。`band_count` 建议为物理核数 × 2（负载均衡）。
    #[must_use]
    pub fn new(
        runtime: tokio::runtime::Runtime,
        band_count: usize,
        style: Arc<NoteStyle>,
        simd: SimdLevel,
    ) -> Self {
        let band_count = band_count.max(1);
        let slots = (0..band_count).map(|_| Arc::new(BandSlot::new())).collect();
        Self {
            runtime,
            band_count,
            slots,
            strip_slot: Arc::new(BandSlot::new()),
            frame: Vec::new(),
            width: 0,
            height: 0,
            simd,
            style,
        }
    }

    /// 窗口尺寸变化。
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height && !self.frame.is_empty() {
            return;
        }
        self.width = width;
        self.height = height;
        self.frame = vec![0; width as usize * height as usize];
        for slot in &self.slots {
            *slot
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(BandBuffer::default());
        }
    }

    /// 渲染一帧并合成到内部 frame；返回统计。
    ///
    /// `document` 与编辑层是同一个 `Arc`；本方法不 clone 音符数据。
    pub fn render(
        &mut self,
        document: Arc<MidiDocument>,
        index: Arc<TrackIndex>,
        camera: &Camera,
        current_track: usize,
        mode: LodMode,
    ) -> RenderStats {
        if self.frame.len() != self.width as usize * self.height as usize {
            self.resize(self.width.max(1), self.height.max(1));
        }
        let width = self.width;
        let height = self.height;
        let canvas_x0 = camera.keyboard_width.ceil() as u32;
        let total_width = width.saturating_sub(canvas_x0);
        if total_width == 0 || height == 0 {
            return RenderStats::default();
        }
        let bands = self.band_count.min(total_width as usize).max(1);
        let band_width = total_width.div_ceil(bands as u32);
        let (key_min, key_max) = camera.visible_key_range();

        let mut ranges = Vec::with_capacity(bands);
        for band_index in 0..bands {
            let x0 = canvas_x0 + band_index as u32 * band_width;
            if x0 >= width {
                break;
            }
            let x1 = (x0 + band_width).min(width);
            ranges.push((x0, x1));
        }

        let style = Arc::clone(&self.style);
        let camera = camera.clone();
        let document_for_tasks = Arc::clone(&document);
        let index_for_tasks = Arc::clone(&index);
        let slots = self.slots.clone();
        let simd = self.simd;
        let mode_copy = mode;
        let ranges_for_tasks = ranges.clone();

        let mut results: Vec<(usize, BandBuffer, BandStats)> = self.runtime.block_on(async {
            let mut set = JoinSet::new();
            for (band_index, &(x0, x1)) in ranges_for_tasks.iter().enumerate() {
                let Some(slot) = slots.get(band_index).cloned() else {
                    continue;
                };
                let job = BandJob {
                    x0,
                    x1,
                    camera: camera.clone(),
                    mode: mode_copy,
                    current_track,
                    style: Arc::clone(&style),
                    index: Arc::clone(&index_for_tasks),
                    document: Arc::clone(&document_for_tasks),
                    simd,
                    key_min,
                    key_max,
                };
                set.spawn(async move {
                    let mut buffer = take_buffer(&slot);
                    let stats = render_band(&job, &mut buffer);
                    (band_index, buffer, stats)
                });
            }
            let mut results = Vec::with_capacity(ranges_for_tasks.len());
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok(item) => results.push(item),
                    Err(error) => {
                        eprintln!("[render] band task failed: {error}");
                    }
                }
            }
            results
        });

        results.sort_unstable_by_key(|(band_index, _, _)| *band_index);
        let mut stats = RenderStats {
            bands: results.len(),
            ..RenderStats::default()
        };
        for (band_index, buffer, band_stats) in results {
            stats.visible_notes += band_stats.visible_notes;
            stats.candidates += band_stats.candidates;
            stats.long_candidates += band_stats.long_candidates;
            stats.per_band.push(band_stats);
            if let Some(&(x0, x1)) = ranges.get(band_index) {
                let expected_width = (x1 - x0) as usize;
                if buffer.width == expected_width && buffer.height == height as usize {
                    for y in 0..height as usize {
                        let src_start = y * buffer.width;
                        let dst_start = y * width as usize + x0 as usize;
                        self.frame[dst_start..dst_start + expected_width]
                            .copy_from_slice(&buffer.pixels[src_start..src_start + expected_width]);
                    }
                }
                if let Some(slot) = self.slots.get(band_index) {
                    put_buffer(slot, buffer);
                }
            }
        }
        stats
    }

    /// 只渲染窗口中的一条 x 区间并直接合成进 frame（滚动增量路径）。
    ///
    /// 这条路径是"滚动期间仍保持完整外观"的关键：已有帧按像素平移，只有新
    /// 露出的窄条重新光栅化；窄条与全量帧使用同一 Exact 光栅化，不省填充/
    /// 描边/音轨。
    #[allow(clippy::too_many_arguments)]
    pub fn render_strip_into_frame(
        &mut self,
        document: Arc<MidiDocument>,
        index: Arc<TrackIndex>,
        camera: &Camera,
        current_track: usize,
        mode: LodMode,
        x0: u32,
        x1: u32,
    ) -> BandStats {
        if x0 >= x1 || x1 > self.width || self.height == 0 {
            return BandStats::default();
        }
        let (key_min, key_max) = camera.visible_key_range();
        let job = BandJob {
            x0,
            x1,
            camera: camera.clone(),
            mode,
            current_track,
            style: Arc::clone(&self.style),
            index: Arc::clone(&index),
            document: Arc::clone(&document),
            simd: self.simd,
            key_min,
            key_max,
        };
        let mut buffer = take_buffer(&self.strip_slot);
        let stats = render_band(&job, &mut buffer);
        let expected_width = (x1 - x0) as usize;
        if buffer.width == expected_width && buffer.height == self.height as usize {
            let width = self.width as usize;
            for y in 0..self.height as usize {
                let src_start = y * buffer.width;
                let dst_start = y * width + x0 as usize;
                self.frame[dst_start..dst_start + expected_width]
                    .copy_from_slice(&buffer.pixels[src_start..src_start + expected_width]);
            }
        }
        put_buffer(&self.strip_slot, buffer);
        stats
    }

    /// 当前合成后的窗口像素。
    #[must_use]
    pub fn frame(&self) -> &[u32] {
        &self.frame
    }

    /// 当前合成后的窗口像素（可变：UI 线程叠加键盘/标尺/HUD）。
    #[must_use]
    pub fn frame_mut(&mut self) -> &mut [u32] {
        &mut self.frame
    }

    /// 窗口宽度。
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 窗口高度。
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// 每轨颜色（编辑层插入预览也要用）。
    #[must_use]
    pub fn style(&self) -> &Arc<NoteStyle> {
        &self.style
    }
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Renderer")
            .field("band_count", &self.band_count)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("simd", &self.simd)
            .finish()
    }
}
