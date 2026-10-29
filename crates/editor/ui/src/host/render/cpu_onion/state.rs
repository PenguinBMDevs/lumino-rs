//! CPU 洋葱皮帧冻结状态 — 只在指纹变化时重新光栅化并递增 `version`。
//!
//! 设计约束（Step 3）：
//! - **整帧冻结**：文档代次 / 当前轨 / 静音集合 / 调色板 / 相机 / 画布尺寸任一变化才
//!   重新 `render_full`；否则复用上一帧与同一 `version`（渲染侧按版本判重上传）。
//! - **整索引重建**：`track_notes_gen` 变化时重建整份 [`TrackIndex`]（Step 5 换增量/脏区，
//!   本步明确不做脏区优化）。
//! - **整帧拷贝**：`CpuFrame.pixels`（`Vec<u8>`）→ `Arc<[u8]>` 一次拷贝（Step 5 做局部上传）。
//! - 文档快照用 `Arc::new(MidiDocument::clone())`：`MidiDocument::clone` 是块级浅拷贝
//!   （音符块 `Arc` 共享 + 写时复制），不复制音符大块。

use std::sync::Arc;

use lumino_cpu_onion::{CpuOnionRenderer, TrackIndex};
use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::MidiDocument;

use super::camera::CpuOnionGeometry;

/// CPU 洋葱皮 band 数上限（每 band 一条 tokio worker 线程）。
///
/// 洋葱皮帧通常只有 chocolate 量级的可见音符，4 band 已能把单帧压到亚毫秒；
/// 再多的线程只增加调度开销与内存（每 band 一块 band 缓冲）。
const MAX_BANDS: usize = 4;

/// 帧冻结点：这些输入任一变化都必须重新光栅化。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuOnionFrameKey {
    /// 文档音符代次（`EditorData::track_notes_gen`）
    pub track_notes_gen: u64,
    /// 当前音轨（CPU 帧跳过它，由 GPU 主轨绘制）
    pub current_track: usize,
    /// 静音集合指纹（顺序无关）
    pub mute_fingerprint: u64,
    /// 当前调色板索引（实例颜色固化在像素里）
    pub palette_idx: u8,
    /// 物理矩形 + 相机（含尺寸、滚动、缩放、键数）
    pub geometry: CpuOnionGeometry,
}

/// 一次 `update` 的产出。
pub(crate) struct CpuOnionOutput {
    /// 待叠加的帧（`None` = 不绘制）
    pub frame: Option<CpuOnionFrame>,
    /// 本帧是否重新光栅化（调用方据此置内容脏 → present 前等待）
    pub rebuilt: bool,
}

/// CPU 洋葱皮运行时状态（渲染器 + 文档快照 + 帧冻结）。
#[derive(Debug, Default)]
pub(crate) struct CpuOnionState {
    /// 渲染器（首次使用时惰性创建，内部持有 tokio 多线程 runtime）
    renderer: Option<CpuOnionRenderer>,
    /// 文档快照（块级浅拷贝，不复制音符大块）
    document: Option<Arc<MidiDocument>>,
    /// 派生索引（当前在 `index_gen` 代次上）
    index: Option<Arc<TrackIndex>>,
    /// 当前帧（`None` = 不绘制）
    frame: Option<CpuOnionFrame>,
    /// 帧冻结点（`None` = 无有效帧）
    frozen: Option<CpuOnionFrameKey>,
    /// 帧内容版本（每次重渲染递增；渲染侧据此判重上传）
    version: u64,
    /// 索引/快照对应的 `track_notes_gen`
    index_gen: Option<u64>,
    /// 渲染器调色板缓存对应的调色板索引
    palette_idx: Option<u8>,
}

impl CpuOnionState {
    /// 本帧不绘制，但保留缓存（相机退化 / 纵向卷帘 / 无文档）。
    pub(crate) fn clear_frame(&mut self) {
        self.frame = None;
        self.frozen = None;
    }

    /// 彻底停用（后端 = GPU）：释放渲染器、文档快照与索引。
    ///
    /// 默认后端就是 GPU，且 CPU 模式可能长期不被使用——不能为了「切回更快」
    /// 而长期驻留一份索引/渲染缓冲。切回 CPU 时重建（一次 O(总音符数)）。
    pub(crate) fn deactivate(&mut self) {
        if self.renderer.is_none() && self.document.is_none() && self.frame.is_none() {
            return;
        }
        tracing::debug!("[cpu-onion] 停用 CPU 洋葱皮（释放渲染器与文档快照）");
        *self = Self::default();
    }

    /// 当前帧（诊断用）。
    #[cfg(test)]
    pub(crate) fn frame(&self) -> Option<&CpuOnionFrame> {
        self.frame.as_ref()
    }

    /// 生成/复用 CPU 洋葱皮帧。
    ///
    /// - 指纹与上一帧一致 → 直接复用（不重渲染、不递增版本、不分配）；
    /// - 文档代次变化 → 重建整份索引 + 文档快照（本步允许，Step 5 换增量）；
    /// - 否则按新相机重渲染一帧并递增版本。
    ///
    /// `muted` 长度 = 文档轨数（越界缺省 `false`）；`current_track` 越界时
    /// 渲染器按「非当前」处理（所有轨都是洋葱皮）。
    pub(crate) fn update(
        &mut self,
        key: CpuOnionFrameKey,
        document: &MidiDocument,
        muted: &[bool],
    ) -> CpuOnionOutput {
        if self.frozen == Some(key) {
            return CpuOnionOutput {
                frame: self.frame.clone(),
                rebuilt: false,
            };
        }

        // 文档代次变化（或首次）→ 重建整份索引与快照
        let rebuild_index = self.index_gen != Some(key.track_notes_gen) || self.index.is_none();
        if rebuild_index {
            let snapshot = Arc::new(document.clone());
            self.index = Some(Arc::new(TrackIndex::build(&snapshot)));
            self.document = Some(snapshot);
            self.index_gen = Some(key.track_notes_gen);
        }

        let (Some(snapshot), Some(index)) = (self.document.clone(), self.index.clone()) else {
            // 无快照/索引：本帧不可绘制（防御性，正常路径不会到达）
            self.frame = None;
            self.frozen = None;
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
            };
        };

        let track_count = snapshot.track_count().max(1);
        // 调色板或轨数变化都要重建每轨颜色缓存（实例颜色固化在像素里）
        let refresh_palette = rebuild_index || self.palette_idx != Some(key.palette_idx);
        let pixels: Arc<[u8]> = {
            let renderer = self
                .renderer
                .get_or_insert_with(|| CpuOnionRenderer::new(worker_bands(), track_count));
            renderer.resize(key.geometry.width, key.geometry.height);
            if refresh_palette {
                renderer.refresh_palette(track_count);
            }
            renderer.render_full(
                snapshot,
                index,
                &key.geometry.camera,
                key.current_track,
                muted,
            );
            // 整帧拷贝（`Vec<u8>` → `Arc<[u8]>`，本步允许；Step 5 做局部上传）
            Arc::from(renderer.frame().pixels.as_slice())
        };

        self.version = self.version.wrapping_add(1).max(1);
        self.palette_idx = Some(key.palette_idx);
        let frame = CpuOnionFrame::new(
            key.geometry.x,
            key.geometry.y,
            key.geometry.width,
            key.geometry.height,
            pixels,
            self.version,
        );
        self.frame = Some(frame.clone());
        self.frozen = Some(key);
        CpuOnionOutput {
            frame: Some(frame),
            rebuilt: true,
        }
    }
}

/// CPU 洋葱皮 band 数：按可用并行度取，限制在 `1..=MAX_BANDS`。
fn worker_bands() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |parallelism| parallelism.get())
        .clamp(1, MAX_BANDS)
}
