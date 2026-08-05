//! 洋葱皮渲染 — 全量流式分块 + 事件级增量上传（UI 线程 → WGPU 线程）
//!
//! 架构（2026-08-05 两轮优化：流式分块 + 事件级增量）：
//! - UI 线程：`stream_onion_skin_instances` 每帧由 `decide_action` 三态决策
//!   - `Full`：全量流式会话（首次 / 音轨进出洋葱皮 / mute / 调色板）——
//!     分块构建 NoteInstance（每块 ≤ 800 万实例 = 128 MB），逐块 send，
//!     每轨末尾强制 flush（空块 = 段表占位），最后 send `Done`
//!   - `Delta`：事件级增量（编辑洋葱皮音轨）——只构建被编辑音轨，send `TrackDelta`
//!   - `None`：无操作（只改当前/静音音轨 → 豁免）
//! - WGPU 线程：render loop 每帧 drain streaming channel
//!   - `Chunk` 流构建音轨段表（track_id → offset/len），`Done` 触发
//!     finish_streaming_upload（更新 cull info）
//!   - `TrackDelta`：等长 → 段内音符级替换；变长 → GPU 内部搬移后续段
//!     （无 CPU 镜像，黑乐谱单音轨海量音符场景不再全量重传）
//!   - GPU 最终持有全量数据（6 亿音符 = 9.6 GB GPU 显存），CPU 无镜像
//!
//! 性能范式：CPU 峰值 ~256-384 MB（旧方案 33 GB）；GPU 全量常驻一次性上传；
//! GPU culling 每帧（cull.wgsl 批量原子剔除）+ Indirect draw（CPU 零参与）
//!
//! 渲染顺序：洋葱皮（不透明 alpha=1.0）→ 主音轨（不透明）→ UI
//! 深度测试：洋葱皮先绘制 depth=0.0，主音轨后绘制 LessEqual 0.0<=0.0 覆盖

use crate::host::Host;
use lumino_editor_state::EditorData;
use lumino_gfx::{NoteInstance, OnionSkinStreamMsg, WgpuRenderThread};

/// 洋葱皮描边宽度（固定 1 像素，与主音轨一致）
const ONION_SKIN_BORDER_WIDTH: u32 = 1;

/// 流式上传分块大小（每块 ≤ 800 万实例 = 128 MB，大块减少传输次数）
const STREAMING_CHUNK_SIZE: usize = 8_000_000;

/// 洋葱皮状态缓存
///
/// 跟踪 track_notes_gen、音轨开关、当前音轨、调色板变化，避免每帧重建全量实例。
/// 字段以值传递（不持有 Host 引用），避免借用冲突。
///
/// `last_current_track` 默认 `usize::MAX`（哨兵值），确保首次 `needs_rebuild`
/// 即使 `current_track == 0` 也会触发重建（`0 != usize::MAX`）。
/// `last_palette_idx` 默认 `u8::MAX`（哨兵值），确保首次或调色板未初始化时触发重建。
pub(crate) struct OnionSkinState {
    /// 上次构建时的 track_notes_gen（用于检测音符数据变化）
    last_track_notes_gen: u64,
    /// 上次构建时的音轨开关指纹（is_muted hash）
    last_mute_fingerprint: u64,
    /// 上次构建时的当前音轨（切换主音轨时需重建）
    last_current_track: usize,
    /// 上次构建时的调色板索引（切换调色板时需重建）
    last_palette_idx: u8,
    /// 是否已初始化（首次上传）
    initialized: bool,
}

/// 洋葱皮构建动作（事件级增量，2026-08-05）
///
/// 由 [`OnionSkinState::decide_action`] 决策：
/// - `None`：无需任何操作（数据未变，或变化全部豁免——只改当前/静音音轨）
/// - `Delta(tracks)`：仅这些音轨需要事件级增量替换（段级替换，只传被编辑音轨）
/// - `Full`：全量重建（首次上传 / 音轨进出洋葱皮 / mute / 调色板 / 未知脏音轨）
#[derive(Debug)]
pub(super) enum OnionSkinAction {
    None,
    Delta(Vec<usize>),
    Full,
}

/// 洋葱皮指纹信息（用于 needs_rebuild 比较）
pub(super) struct OnionSkinFingerprint {
    track_gen: u64,
    mute_fp: u64,
    current_track: usize,
    palette_idx: u8,
    /// 本次 `track_gen` 变化明确影响的音轨集合（`None` = 未知→保守全量重建）
    ///
    /// 增量优化（2026-08-05）：编辑操作（如 `mark_current_track_changed`）记录
    /// 精确影响的音轨。洋葱皮不显示当前音轨与静音音轨——当变化音轨全部落在
    /// 该集合内时，洋葱皮 GPU 数据实际未变，可豁免每帧全量重建上传。
    onion_dirty_tracks: Option<std::collections::HashSet<usize>>,
    /// 当前静音音轨集合（洋葱皮跳过，参与豁免判断）
    muted_tracks: Vec<usize>,
}

impl Default for OnionSkinState {
    fn default() -> Self {
        Self {
            last_track_notes_gen: 0,
            last_mute_fingerprint: 0,
            last_current_track: usize::MAX,
            last_palette_idx: u8::MAX,
            initialized: false,
        }
    }
}

impl OnionSkinState {
    /// 计算音轨开关指纹（is_muted 状态 hash）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用 / 不得限制音轨数量。
    /// 旧实现用 u64 位掩码仅支持 64 轨（超出 break 截断），
    /// 新实现用 hash 支持任意音轨数量。
    fn compute_mute_fingerprint(host: &Host) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for track in host.root.sidebar.tracks.iter() {
            track.is_muted.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// 收集当前重建所需的指纹信息（避免在 needs_rebuild 中持有 Host 借用）
    pub(super) fn collect_fingerprint(host: &Host) -> OnionSkinFingerprint {
        let data = &host.root.editor.editor_state.data;
        let muted_tracks = host
            .root
            .sidebar
            .tracks
            .iter()
            .filter(|t| t.is_muted)
            .map(|t| t.id)
            .collect();
        OnionSkinFingerprint {
            track_gen: data.track_notes_gen,
            mute_fp: Self::compute_mute_fingerprint(host),
            current_track: data.current_track,
            palette_idx: lumino_extras::palette::current_palette_idx(),
            onion_dirty_tracks: data.onion_dirty_tracks.clone(),
            muted_tracks,
        }
    }

    /// 决策本次洋葱皮构建动作（三态，事件级增量）
    /// - 布局变化（mute/current/palette）→ `Full`（音轨进出洋葱皮，段表失效）
    /// - 音符数据变化：脏音轨全豁免 → `None`；含洋葱皮音轨 → `Delta(洋葱皮音轨)`；
    ///   未知来源 → `Full`（保守正确性）
    /// - 无变化 → `None`
    pub(super) fn decide_action(&self, fp: &OnionSkinFingerprint) -> OnionSkinAction {
        if !self.initialized {
            return OnionSkinAction::Full;
        }

        // 布局变化 → 全量重建（段表失效，增量无法安全应用）
        if fp.mute_fp != self.last_mute_fingerprint
            || fp.current_track != self.last_current_track
            || fp.palette_idx != self.last_palette_idx
        {
            return OnionSkinAction::Full;
        }

        if fp.track_gen != self.last_track_notes_gen {
            match &fp.onion_dirty_tracks {
                Some(dirty) if !dirty.is_empty() => {
                    // 从脏音轨中挑出洋葱皮音轨（非当前、非静音）
                    let targets: Vec<usize> = dirty
                        .iter()
                        .filter(|t| **t != fp.current_track && !fp.muted_tracks.contains(t))
                        .copied()
                        .collect();
                    if targets.is_empty() {
                        // 变化全部豁免（只改当前/静音音轨）→ 无操作
                        return OnionSkinAction::None;
                    }
                    return OnionSkinAction::Delta(targets);
                }
                // None（未知来源）或空集合 → 保守全量重建
                _ => return OnionSkinAction::Full,
            }
        }

        OnionSkinAction::None
    }

    /// 标记已构建（在重建后调用）
    pub(super) fn mark_built(&mut self, fp: &OnionSkinFingerprint) {
        self.last_track_notes_gen = fp.track_gen;
        self.last_mute_fingerprint = fp.mute_fp;
        self.last_current_track = fp.current_track;
        self.last_palette_idx = fp.palette_idx;
        self.initialized = true;
    }
}

impl Host {
    /// 洋葱皮实例构建入口（流式分块 / 事件级增量）
    ///
    /// 每帧调用，由 [`OnionSkinState::decide_action`] 决策三种路径：
    /// - `None`：无操作（主音轨编辑豁免 + 未变化）
    /// - `Full`：全量流式会话（首次 / 布局变化）——分块构建 NoteInstance
    ///   每块 ≤ `STREAMING_CHUNK_SIZE`（800 万实例 = 128 MB），立即 send 到
    ///   WGPU 线程的 streaming channel，最后 send `Done`
    /// - `Delta`：事件级增量——只构建被编辑的洋葱皮音轨，send `TrackDelta`
    ///
    /// 2026-08 单一权威源：音符数据一律从 `document` 读取（track_notes 缓存已删除）。
    pub(super) fn stream_onion_skin_instances(&mut self) {
        // 走带模式跳过
        if self.root.is_arrangement_mode() {
            return;
        }

        let fp = OnionSkinState::collect_fingerprint(self);
        let action = self.render_ctx.onion_skin_state.decide_action(&fp);

        if matches!(action, OnionSkinAction::None) {
            return;
        }

        // 获取 WGPU 线程引用（用于 send 消息）
        let Some(wgpu_thread) = self.render_ctx.wgpu_render_thread.as_ref() else {
            tracing::warn!("stream_onion_skin_instances: wgpu_render_thread is None");
            return;
        };

        match action {
            OnionSkinAction::None => {}
            OnionSkinAction::Full => self.stream_onion_skin_full(&fp, wgpu_thread),
            OnionSkinAction::Delta(tracks) => {
                self.stream_onion_skin_delta(&fp, wgpu_thread, &tracks)
            }
        }

        // 标记已构建（None / Full / Delta 三路都更新指纹，防止重复构建）
        self.render_ctx.onion_skin_state.mark_built(&fp);
    }

    /// 全量流式会话：分块构建所有洋葱皮音轨实例并 send
    /// 每块 ≤ 800 万实例（128 MB），sync_channel(3) 背压；CPU 峰值 ~256-384 MB。
    /// 每轨末尾强制 flush（空块 = 段表占位）：WGPU 侧据此构建音轨段表，
    /// 事件级增量（TrackDelta）依赖它定位段。
    fn stream_onion_skin_full(&self, fp: &OnionSkinFingerprint, wgpu_thread: &WgpuRenderThread) {
        let data = &self.root.editor.editor_state.data;
        let current_track = data.current_track;
        let tracks = &self.root.sidebar.tracks;
        let border_width = ONION_SKIN_BORDER_WIDTH;

        // 辅助闭包：判断音轨是否静音
        let is_track_muted = |track_id: usize| -> bool {
            tracks
                .iter()
                .find(|t| t.id == track_id)
                .is_some_and(|t| t.is_muted)
        };

        // 分块构建 + send 的辅助闭包（每轨末尾必 flush，空块 = 段表占位）
        let mut chunk: Vec<NoteInstance> = Vec::with_capacity(STREAMING_CHUNK_SIZE);
        let total_counter = std::cell::Cell::new(0usize);

        let flush_chunk =
            |chunk: &mut Vec<NoteInstance>, track_id: usize, wgpu_thread: &WgpuRenderThread| {
                let instances = std::mem::replace(chunk, Vec::with_capacity(STREAMING_CHUNK_SIZE));
                total_counter.set(total_counter.get() + instances.len());
                wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Chunk {
                    track_id,
                    instances,
                });
            };

        // 从 MidiDocument 构建所有洋葱皮音轨（单一权威源）
        if let Some(doc) = data.document.as_ref() {
            let track_count = doc.track_count();
            for track_idx in 0..track_count {
                if track_idx == current_track || is_track_muted(track_idx) {
                    continue;
                }
                let doc_notes = doc.track_notes(track_idx);
                let color = lumino_extras::palette::current_track_color_f32(track_idx);
                for ne in doc_notes.iter() {
                    chunk.push(NoteInstance::new(
                        ne.start_tick as f32,
                        ne.key,
                        (ne.end_tick - ne.start_tick) as f32,
                        color,
                        border_width,
                    ));
                    if chunk.len() >= STREAMING_CHUNK_SIZE {
                        flush_chunk(&mut chunk, track_idx, wgpu_thread);
                    }
                }
                flush_chunk(&mut chunk, track_idx, wgpu_thread);
            }
        }

        // 发送完成标志（WGPU 侧 finish_streaming_upload + 段表确认）
        wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Done);

        tracing::info!(
            "[onion-skin] 流式上传完成：{} 个实例 (track_gen={}, current_track={}, palette_idx={})",
            total_counter.get(),
            fp.track_gen,
            fp.current_track,
            fp.palette_idx
        );
    }

    /// 事件级增量：只构建被编辑的洋葱皮音轨并 send `TrackDelta`
    /// 黑乐谱核心路径：编辑非主音轨只重传该音轨（等长=音符级增量；
    /// 变长=WGPU 侧 GPU 搬移后续段），不再全量重建。
    /// `tracks` 来自 decide_action，已过滤为洋葱皮音轨且布局未变，
    /// 保证 WGPU 段表与集合一致，TrackDelta 必然命中段。
    fn stream_onion_skin_delta(
        &self,
        fp: &OnionSkinFingerprint,
        wgpu_thread: &WgpuRenderThread,
        tracks: &[usize],
    ) {
        let data = &self.root.editor.editor_state.data;
        let border_width = ONION_SKIN_BORDER_WIDTH;

        for &track_id in tracks {
            let instances = build_track_instances(data, track_id, border_width);
            wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::TrackDelta {
                track_id,
                instances,
            });
        }

        tracing::debug!(
            "[onion-skin] 事件级增量：更新 {} 个音轨 {:?} (track_gen={})",
            tracks.len(),
            tracks,
            fp.track_gen
        );
    }
}

/// 构建单音轨的完整 NoteInstance 列表（数据源：document 单一权威源）
///
/// 2026-08 改造：track_notes 缓存已删除，一律从 `MidiDocument` 读。
/// 无文档 → 空列表。
fn build_track_instances(
    data: &EditorData,
    track_id: usize,
    border_width: u32,
) -> Vec<NoteInstance> {
    let color = lumino_extras::palette::current_track_color_f32(track_id);

    if let Some(doc) = data.document.as_ref() {
        let doc_notes = doc.track_notes(track_id);
        return doc_notes
            .iter()
            .map(|ne| {
                NoteInstance::new(
                    ne.start_tick as f32,
                    ne.key,
                    (ne.end_tick - ne.start_tick) as f32,
                    color,
                    border_width,
                )
            })
            .collect();
    }

    Vec::new()
}

/// 状态机三态决策测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "onion_skin/tests.rs"]
mod tests;
