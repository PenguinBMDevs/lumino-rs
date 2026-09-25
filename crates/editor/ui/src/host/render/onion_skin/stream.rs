//! 洋葱皮流式上传实现（全量会话 + 事件级增量）
//!
//! 被父模块 [`super::stream_onion_skin_instances`] 按决策结果调用：
//! - `stream_onion_skin_full`：全量分块构建所有音轨实例并 send
//! - `stream_onion_skin_delta`：事件级增量，只构建被编辑音轨并 send `TrackDelta`

use crate::host::Host;
use lumino_editor_state::EditorData;
use lumino_gfx::{NoteInstance, OnionSkinStreamMsg, WgpuRenderThread};

use super::{OnionSkinFingerprint, PARALLEL_BUILD_MIN, STREAMING_CHUNK_SIZE, onion_border_width};

impl Host {
    /// 全量流式会话：分块构建**所有音轨**（含当前轨、静音轨）实例并 send
    ///
    /// 统一全量渲染（2026-08-06）：GPU buffer 常驻所有轨全部音符——
    /// 当前轨（主音轨段，shader 按 ViewState uniform 染主轨色/深度 0）与
    /// 静音轨（shader 按静音位图隐藏，仅主轨身份时显示）都包含在内。
    /// 切轨/静音变化因此零重传（只发 ViewState uniform）。
    ///
    /// 每块 ≤ 800 万实例（128 MB），sync_channel(3) 背压；CPU 峰值 ~256-384 MB。
    /// 每轨末尾强制 flush（空块 = 段表占位）：WGPU 侧据此构建音轨段表，
    /// 事件级增量（TrackDelta / 段内 NoteEvent）依赖它定位段。
    pub(super) fn stream_onion_skin_full(
        &self,
        fp: &OnionSkinFingerprint,
        wgpu_thread: &WgpuRenderThread,
    ) {
        let data = &self.root.editor.editor_state.data;

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

        // 预分配：统计所有轨实例总数（ChunkedList len O(1)），
        // 消除流式 append 2× 倍增的容量余量（2.9 亿音符省 ~4GB GPU 显存）
        let total: usize = data
            .document
            .as_ref()
            .map(|doc| {
                (0..doc.track_count())
                    .map(|t| doc.track_notes(t).len())
                    .sum()
            })
            .unwrap_or(0);
        if total > 0 {
            wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Reserve { total });
        }

        // 从 MidiDocument 构建所有音轨（单一权威源；含当前轨与静音轨）
        if let Some(doc) = data.document.as_ref() {
            let track_count = doc.track_count();
            for track_idx in 0..track_count {
                let doc_notes = doc.track_notes(track_idx);
                let color = lumino_extras::palette::current_track_color_f32(track_idx);
                // 轨道索引编码进 border_width 高 16 位（统一编码：主音轨判定 + 稳定深度）
                let border_width = onion_border_width(track_idx);
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

        tracing::debug!(
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
    pub(super) fn stream_onion_skin_delta(
        &self,
        fp: &OnionSkinFingerprint,
        wgpu_thread: &WgpuRenderThread,
        tracks: &[usize],
    ) {
        let data = &self.root.editor.editor_state.data;

        for &track_id in tracks {
            let parts = build_track_instance_parts(data, track_id);
            wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::TrackDelta { track_id, parts });
        }

        tracing::debug!(
            "[onion-skin] 事件级增量：更新 {} 个音轨 {:?} (track_gen={})",
            tracks.len(),
            tracks,
            fp.track_gen
        );
    }
}

/// 构建单音轨的完整 NoteInstance 列表（分片；数据源：document 单一权威源）
///
/// 2026-08 改造：track_notes 缓存已删除，一律从 `MidiDocument` 读。
/// 无文档 → 空列表。
///
/// 2026-09 并行构建：大轨（≥ [`PARALLEL_BUILD_MIN`]）按索引区间切分，多线程
/// `iter_window` 顺序构建后**按序分片返回**（不做二次拼接拷贝）——单轨 3000W
/// 实例构建实测 ~180ms，是主轨段重建（`TrackDelta`）的主要 CPU 成本。
/// 小轨返回单分片（调用方语义不变）。
fn build_track_instance_parts(data: &EditorData, track_id: usize) -> Vec<Vec<NoteInstance>> {
    let color = lumino_extras::palette::current_track_color_f32(track_id);
    // 轨道索引编码进 border_width 高 16 位（稳定深度优先级）
    let border_width = onion_border_width(track_id);

    let Some(doc) = data.document.as_ref() else {
        return Vec::new();
    };
    let doc_notes = doc.track_notes(track_id);
    let total = doc_notes.len();
    if total == 0 {
        return Vec::new();
    }

    if total < PARALLEL_BUILD_MIN {
        let mut v = Vec::with_capacity(total);
        for ne in doc_notes.iter() {
            v.push(NoteInstance::new(
                ne.start_tick as f32,
                ne.key,
                (ne.end_tick - ne.start_tick) as f32,
                color,
                border_width,
            ));
        }
        return vec![v];
    }

    use rayon::prelude::*;
    let workers = rayon::current_num_threads().max(1);
    let part = total.div_ceil(workers);
    (0..workers)
        .into_par_iter()
        .map(|w| {
            let lo = (w * part).min(total);
            let hi = ((w + 1) * part).min(total);
            // 精确预分配：iter_window 无 size_hint，逐元素 push 会触发倍增重分配
            let mut v = Vec::with_capacity(hi.saturating_sub(lo));
            for (_, ne) in doc_notes.iter_window(lo, hi) {
                v.push(NoteInstance::new(
                    ne.start_tick as f32,
                    ne.key,
                    (ne.end_tick - ne.start_tick) as f32,
                    color,
                    border_width,
                ));
            }
            v
        })
        .collect()
}
