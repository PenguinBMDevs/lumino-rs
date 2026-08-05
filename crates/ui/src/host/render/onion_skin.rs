//! 洋葱皮渲染 — MIDI 加载后流式分块构建所有音轨音符，通过 streaming channel 传到 WGPU 线程
//!
//! 架构（分离渲染线程模式 + 流式分块上传，2026-08-05 优化）：
//! - UI 线程：stream_onion_skin_instances 检测 track_notes_gen/mute/current_track/palette 变化
//!   - 变化时分块构建 NoteInstance（每块 ≤ 800 万实例 = 128 MB）
//!   - 每块构建完立即 send 到 WGPU 线程的 sync_channel(3)，不积累全量 Vec
//!   - 全部构建完后 send 空 Vec（完成标志）
//! - WGPU 线程：render loop 每帧 drain streaming channel
//!   - 首块触发 begin_streaming_upload
//!   - 每块调用 streaming_append 直接 write_buffer 到 GPU（不维护 CPU 副本）
//!   - 空 Vec 触发 finish_streaming_upload（更新 cull info）
//!   - GPU 最终持有全量数据（6 亿音符 = 9.6 GB GPU 显存）
//!
//! 性能范式：
//! - CPU 峰值 ~256-384 MB（旧方案 33 GB：collected 14.4 GB + instances Vec 9.6 GB + GpuNoteBuffer.instances 9.6 GB）
//! - GPU 全量常驻（一次性上传，非每帧重写）
//! - GPU culling 每帧（复用 cull.wgsl 的 workgroup 批量原子剔除）
//! - Indirect draw（CPU 零参与绘制提交）
//!
//! 渲染顺序：洋葱皮（不透明 alpha=1.0）→ 主音轨（不透明）→ UI
//! 深度测试：洋葱皮先绘制 depth=0.0，主音轨后绘制 LessEqual 0.0<=0.0 覆盖

use crate::host::Host;
use lumino_gfx::NoteInstance;

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

    /// 检查是否需要重建洋葱皮实例
    pub(super) fn needs_rebuild(&self, fp: &OnionSkinFingerprint) -> bool {
        if !self.initialized {
            return true;
        }

        if fp.track_gen != self.last_track_notes_gen {
            // 音符数据变化：仅当变化明确落在洋葱皮不显示的音轨（当前音轨/静音音轨）时豁免，
            // 否则全量重建上传。None（未知）或空集合均保守重建，保证正确性。
            let affects_onion_skin = match &fp.onion_dirty_tracks {
                Some(dirty) if !dirty.is_empty() => dirty
                    .iter()
                    .any(|t| *t != fp.current_track && !fp.muted_tracks.contains(t)),
                _ => true,
            };
            if affects_onion_skin {
                return true;
            }
        }

        fp.mute_fp != self.last_mute_fingerprint
            || fp.current_track != self.last_current_track
            || fp.palette_idx != self.last_palette_idx
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
    /// 流式分块构建洋葱皮 NoteInstance 并 send 到 WGPU 线程
    ///
    /// 检测到 needs_rebuild 时，遍历所有音轨（除当前主音轨），分块构建 NoteInstance，
    /// 每块 ≤ `STREAMING_CHUNK_SIZE`（800 万实例 = 128 MB），立即 send 到 WGPU 线程的
    /// streaming channel。全部构建完后 send 空 Vec（完成标志）。
    ///
    /// CPU 峰值 = STREAMING_CHUNK_SIZE × 16 B + channel 在途 = ~256-384 MB（旧方案 33 GB）。
    /// sync_channel(3) 背压：channel 满时阻塞 UI 线程，等 WGPU 线程消费。
    ///
    /// 数据源融合（与 arrangement_ops/clipboard.rs + selection.rs 范式一致）：
    /// 1. 优先从 `track_notes` 缓存读（已编辑的音轨，含 undo/redo 状态）
    /// 2. `track_notes` 未缓存的音轨从 `Arc<MidiDocument>` 读（未编辑的原始音轨）
    pub(super) fn stream_onion_skin_instances(&mut self) {
        // 走带模式跳过
        if self.root.is_arrangement_mode() {
            return;
        }

        let fp = OnionSkinState::collect_fingerprint(self);
        let needs_rebuild = self.render_ctx.onion_skin_state.needs_rebuild(&fp);

        if !needs_rebuild {
            return;
        }

        // 获取 WGPU 线程引用（用于 send chunk）
        let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread else {
            tracing::warn!("stream_onion_skin_instances: wgpu_render_thread is None");
            return;
        };

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

        // 分块构建 + send 的辅助闭包
        let mut chunk: Vec<NoteInstance> = Vec::with_capacity(STREAMING_CHUNK_SIZE);
        let total_counter = std::cell::Cell::new(0usize);

        let flush_chunk = |chunk: &mut Vec<NoteInstance>,
                           wgpu_thread: &lumino_gfx::WgpuRenderThread| {
            if chunk.is_empty() {
                return;
            }
            let chunk_to_send = std::mem::replace(chunk, Vec::with_capacity(STREAMING_CHUNK_SIZE));
            total_counter.set(total_counter.get() + chunk_to_send.len());
            wgpu_thread.send_onion_skin_chunk(chunk_to_send);
        };

        // 1. 从 track_notes 缓存构建（已编辑的音轨）
        for (track_id, notes) in data.track_notes.iter() {
            if *track_id == current_track || is_track_muted(*track_id) {
                continue;
            }
            let color = lumino_extras::palette::current_track_color_f32(*track_id);
            for note in notes.iter() {
                chunk.push(NoteInstance::new(
                    note.tick,
                    note.key as u8,
                    note.length,
                    color,
                    border_width,
                ));
                if chunk.len() >= STREAMING_CHUNK_SIZE {
                    flush_chunk(&mut chunk, wgpu_thread);
                }
            }
        }

        // 2. 从 MidiDocument 构建未缓存到 track_notes 的音轨（未编辑的原始音轨）
        if let Some(doc) = data.document.as_ref() {
            let track_count = doc.track_count();
            for track_idx in 0..track_count {
                if track_idx == current_track || is_track_muted(track_idx) {
                    continue;
                }
                // 已在 track_notes 缓存中的音轨跳过（避免重复）
                if data.track_notes.contains_key(&track_idx) {
                    continue;
                }
                let doc_notes = doc.track_notes(track_idx);
                if doc_notes.is_empty() {
                    continue;
                }
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
                        flush_chunk(&mut chunk, wgpu_thread);
                    }
                }
            }
        }

        // 发送最后一块
        flush_chunk(&mut chunk, wgpu_thread);

        // 发送完成标志（空 Vec）
        wgpu_thread.send_onion_skin_chunk(Vec::new());

        // 标记已构建
        self.render_ctx.onion_skin_state.mark_built(&fp);

        tracing::info!(
            "[onion-skin] 流式上传完成：{} 个实例 (track_gen={}, current_track={}, palette_idx={})",
            total_counter.get(),
            fp.track_gen,
            fp.current_track,
            fp.palette_idx
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助函数：构造测试用指纹
    fn make_fp(
        track_gen: u64,
        mute_fp: u64,
        current_track: usize,
        palette_idx: u8,
    ) -> OnionSkinFingerprint {
        OnionSkinFingerprint {
            track_gen,
            mute_fp,
            current_track,
            palette_idx,
            onion_dirty_tracks: None,
            muted_tracks: Vec::new(),
        }
    }

    /// 构造带音轨级脏标记的指纹
    fn make_fp_dirty(
        track_gen: u64,
        current_track: usize,
        dirty_tracks: std::collections::HashSet<usize>,
        muted_tracks: Vec<usize>,
    ) -> OnionSkinFingerprint {
        OnionSkinFingerprint {
            track_gen,
            mute_fp: 0,
            current_track,
            palette_idx: 0,
            onion_dirty_tracks: Some(dirty_tracks),
            muted_tracks,
        }
    }

    #[test]
    fn onion_skin_state_default_uninitialized() {
        let state = OnionSkinState::default();
        assert!(!state.initialized);
        assert_eq!(state.last_track_notes_gen, 0);
        assert_eq!(state.last_mute_fingerprint, 0);
        assert_eq!(state.last_current_track, usize::MAX);
        assert_eq!(state.last_palette_idx, u8::MAX);
    }

    #[test]
    fn onion_skin_state_needs_rebuild_on_first_run() {
        let state = OnionSkinState::default();
        assert!(state.needs_rebuild(&make_fp(0, 0, 0, 0)));
    }

    #[test]
    fn onion_skin_state_no_rebuild_after_mark_built() {
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0b1010, 3, 1));
        assert!(!state.needs_rebuild(&make_fp(42, 0b1010, 3, 1)));
    }

    #[test]
    fn onion_skin_state_rebuild_on_gen_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 0, 0));
        assert!(state.needs_rebuild(&make_fp(43, 0, 0, 0)));
    }

    #[test]
    fn onion_skin_state_rebuild_on_mute_change() {
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0b0000, 0, 0));
        assert!(state.needs_rebuild(&make_fp(42, 0b0001, 0, 0)));
    }

    #[test]
    fn onion_skin_state_rebuild_on_track_switch() {
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        assert!(state.needs_rebuild(&make_fp(42, 0, 2, 0)));
    }

    #[test]
    fn onion_skin_state_rebuild_on_palette_switch() {
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 0, 1));
        assert!(state.needs_rebuild(&make_fp(42, 0, 0, 2)));
    }

    // ── 增量豁免测试（编辑主音轨不再全量重建上传） ──────────────────────────

    #[test]
    fn onion_skin_state_skip_rebuild_when_only_current_track_dirty() {
        // 编辑当前音轨 → 洋葱皮不显示该音轨 → 数据未变 → 豁免全量重建上传
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
        assert!(!state.needs_rebuild(&fp));
    }

    #[test]
    fn onion_skin_state_skip_rebuild_consecutive_edits_same_track() {
        // 连续编辑当前音轨（拖动热路径每帧触发）应持续豁免，不累积重建
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        for g in 43..48 {
            let fp = make_fp_dirty(g, 1, std::collections::HashSet::from([1]), vec![]);
            assert!(!state.needs_rebuild(&fp), "gen={g} 不应触发重建");
        }
    }

    #[test]
    fn onion_skin_state_skip_rebuild_when_dirty_track_muted() {
        // 变化音轨是静音音轨 → 洋葱皮不显示 → 豁免
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 2, 0));
        let fp = make_fp_dirty(43, 2, std::collections::HashSet::from([0]), vec![0]);
        assert!(!state.needs_rebuild(&fp));
    }

    #[test]
    fn onion_skin_state_rebuild_when_other_track_dirty() {
        // 编辑了非当前音轨 → 洋葱皮显示它 → 必须重建
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
        assert!(state.needs_rebuild(&fp));
    }

    #[test]
    fn onion_skin_state_rebuild_when_dirty_unknown() {
        // 变化来源未知（None）→ 保守全量重建
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        assert!(state.needs_rebuild(&make_fp(43, 0, 1, 0)));
    }

    #[test]
    fn onion_skin_state_rebuild_on_track_switch_after_skipped_dirty() {
        // 豁免后切换当前音轨 → 音轨切换本身必须触发重建（用最新数据兜底被豁免的编辑）
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        // 豁免一次（编辑音轨1，当前也是1）
        let fp_skip = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
        assert!(!state.needs_rebuild(&fp_skip));
        // 切换到音轨2 → 必须重建（旧被豁免的编辑此时成为洋葱皮数据）
        let fp_switch = make_fp_dirty(43, 2, std::collections::HashSet::from([1]), vec![]);
        assert!(state.needs_rebuild(&fp_switch));
    }

    #[test]
    fn onion_skin_state_rebuild_when_mute_changes_after_skipped_dirty() {
        // 豁免 gen 变更后 mute 状态变化 → 仍须重建
        let mut state = OnionSkinState::default();
        state.mark_built(&make_fp(42, 0, 1, 0));
        let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
        let mut rebuilt = fp;
        rebuilt.mute_fp = 999; // 模拟 mute 状态变化
        assert!(state.needs_rebuild(&rebuilt));
    }
}
