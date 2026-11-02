//! 洋葱皮流式上传实现（全量会话 + 轨布局 + 事件级增量）
//!
//! 被 [`super::Host::stream_onion_skin_instances`] 按决策结果调用：
//! - `stream_onion_skin_full`：全量分块构建音轨实例并 send
//!   - `only_current = None`：**所有**音轨（GPU 全轨常驻，默认行为）
//!   - `only_current = Some(t)`：**只**当前轨（CPU 单轨模式，Q3 B）
//! - `stream_track_layout`：轨数变化 → `TrackLayout` + 新增轨 `TrackDelta`
//! - `stream_onion_skin_delta`：事件级增量，只构建被编辑音轨并 send `TrackDelta`

use std::cell::Cell;

use crate::host::Host;
use lumino_editor_state::EditorData;
use lumino_gfx::{NoteInstance, OnionSkinStreamMsg, WgpuRenderThread};
use lumino_midi_model::MidiDocument;

use super::{OnionSkinFingerprint, STREAMING_CHUNK_SIZE, onion_border_width};

mod instances;

use instances::build_track_instance_parts;

impl Host {
    /// 全量流式会话：分块构建目标音轨的实例并 send。
    ///
    /// `only_current = None`（GPU 后端，默认）：构建**所有音轨**（含当前轨、静音轨）。
    /// 统一全量渲染（2026-08-06）：GPU buffer 常驻所有轨全部音符——当前轨（主音轨段，
    /// shader 按 ViewState uniform 染主轨色/深度 0）与静音轨（shader 按静音位图隐藏，
    /// 仅主轨身份时显示）都包含在内；切轨/静音变化因此零重传（只发 ViewState uniform）。
    ///
    /// `only_current = Some(track)`（CPU 后端，Q3 B）：**只**发当前轨一段——
    /// 非当前轨/静音轨由 CPU 叠加层绘制，不进 GPU 显存：
    /// 1. `BeginSession` 清空段表；
    /// 2. 只按当前轨长度 `Reserve`；
    /// 3. 只发当前轨 `Chunk`（即使 0 条也发空块占位段，主轨事件增量才能命中段）；
    /// 4. `Done`；
    /// 5. `SetViewState { current_track: current+1, muted_tracks }`。
    ///
    /// 当前轨越界（文档轨数不足）时安全清空：不发 `Chunk`，段表保持空，
    /// 仍发 `Done` + `SetViewState`，渲染侧无段可画（不 panic、不脏数据）。
    ///
    /// 每块 ≤ 800 万实例（128 MB），有界通道 `bounded(3)` + 发送侧
    /// `send_timeout` 背压（REND-019 PR-A′）；CPU 峰值 ~256-384 MB。
    /// 每轨末尾强制 flush（空块 = 段表占位）：WGPU 侧据此构建音轨段表，
    /// 事件级增量（TrackDelta / 段内 NoteEvent）依赖它定位段。
    ///
    /// REND-019 PR-A′：会话内任一条消息未送达（超时/断开）即**中止会话**
    /// （不再发送后续 Chunk/Done）——残缺会话会让渲染侧段表错乱；中止后
    /// 降级标志已置位，由 `force_full_next` 节流全量补发完整会话。
    pub(super) fn stream_onion_skin_full(
        &self,
        fp: &OnionSkinFingerprint,
        wgpu_thread: &WgpuRenderThread,
        only_current: Option<usize>,
    ) {
        let t_start = std::time::Instant::now();
        let data = &self.root.editor.editor_state.data;

        // 显式会话边界：清空段表 + 重置计数（空文档会话也生效，防旧段表残留）
        if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::BeginSession) {
            tracing::warn!("[onion-skin] 全量会话中止：BeginSession 未送达（降级补发接管）");
            return;
        }

        // 会话中止标志：任一消息未送达即置位，阻断后续发送与 Done
        let aborted = Cell::new(false);
        let total_counter = Cell::new(0usize);

        let sent_total = match only_current {
            Some(track_id) => self.send_current_track_session(
                data,
                wgpu_thread,
                track_id,
                &aborted,
                &total_counter,
            ),
            None => self.send_all_tracks_session(data, wgpu_thread, &aborted, &total_counter),
        };
        let Some(sent_total) = sent_total else {
            tracing::warn!(
                "[onion-skin] 全量会话中止：消息未送达（已置降级，等待全量补发）已发送 {} 个实例",
                total_counter.get()
            );
            return;
        };

        // 发送完成标志（WGPU 侧 finish_streaming_upload + 段表确认）
        if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Done) {
            tracing::warn!("[onion-skin] 全量会话中止：Done 未送达（降级补发接管）");
            return;
        }

        // CPU 单轨会话：段表只有当前轨一条，完成后同步 ViewState（含越界安全清空）
        if let Some(track_id) = only_current
            && !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::SetViewState {
                current_track: track_id as u32 + 1,
                muted_tracks: fp.muted_tracks.clone(),
            })
        {
            tracing::warn!("[onion-skin] CPU 单轨 ViewState 未送达（切轨状态可能滞后至下次同步）");
        }

        tracing::debug!(
            "[onion-skin] 流式上传完成：{} 个实例 (track_gen={}, current_track={}, palette_idx={}, only_current={:?})",
            sent_total,
            fp.track_gen,
            fp.current_track,
            fp.palette_idx,
            only_current
        );
        // REND-003：全量会话在主线程分块构建 + 背压发送（bounded(3) + send_timeout，
        // REND-019 PR-A′），大工程下是加载后"未响应"的第二段风险，耗时打点便于定位与后续优化。
        tracing::info!(
            "[onion-skin] 全量会话完成：{} 个实例，耗时 {:?}",
            sent_total,
            t_start.elapsed()
        );
    }

    /// GPU 全轨会话：预分配全部轨实例总数后逐轨分块发送。
    ///
    /// 返回 `None` 表示会话被中止（消息未送达）。
    fn send_all_tracks_session(
        &self,
        data: &EditorData,
        wgpu_thread: &WgpuRenderThread,
        aborted: &Cell<bool>,
        total_counter: &Cell<usize>,
    ) -> Option<usize> {
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
        if total > 0 && !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Reserve { total }) {
            tracing::warn!("[onion-skin] 全量会话中止：Reserve 未送达（降级补发接管）");
            return None;
        }

        // 从 MidiDocument 构建所有音轨（单一权威源；含当前轨与静音轨）
        if let Some(doc) = data.document.as_ref() {
            for track_idx in 0..doc.track_count() {
                if aborted.get() {
                    return None;
                }
                send_track_chunked(doc, track_idx, wgpu_thread, aborted, total_counter);
            }
        }
        if aborted.get() {
            return None;
        }
        Some(total_counter.get())
    }

    /// CPU 单轨会话：只预分配 + 只发送当前轨。
    ///
    /// 当前轨越界时**不发送任何 `Chunk`**（`BeginSession` 已清空段表 →
    /// 渲染侧无段可画），返回 `Some(0)` 让调用方继续发 `Done` / `SetViewState`。
    fn send_current_track_session(
        &self,
        data: &EditorData,
        wgpu_thread: &WgpuRenderThread,
        track_id: usize,
        aborted: &Cell<bool>,
        total_counter: &Cell<usize>,
    ) -> Option<usize> {
        let Some((track_id, note_count)) = current_only_plan(data.document.as_ref(), track_id)
        else {
            tracing::debug!(
                "[onion-skin] CPU 单轨会话：当前轨 {track_id} 越界（文档无该轨），清空段表"
            );
            return Some(0);
        };
        if note_count > 0
            && !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Reserve { total: note_count })
        {
            tracing::warn!("[onion-skin] CPU 单轨会话中止：Reserve 未送达（降级补发接管）");
            return None;
        }
        let Some(doc) = data.document.as_ref() else {
            return Some(0);
        };
        send_track_chunked(doc, track_id, wgpu_thread, aborted, total_counter);
        if aborted.get() {
            return None;
        }
        Some(total_counter.get())
    }

    /// 轨数变化增量同步：发送 `TrackLayout`（增删段表）+ 新增轨内容。
    ///
    /// 增长时为新轨补发 `TrackDelta`（新轨通常为空/少量音符；`apply_track_restored`
    /// 恢复的音符也在此补齐），既有轨零重建。返回已发送 `TrackDelta` 的音轨 id，
    /// 供调用方与主轨段重建去重。
    pub(super) fn stream_track_layout(
        &self,
        wgpu_thread: &WgpuRenderThread,
        track_count: usize,
        old_count: usize,
    ) -> Vec<usize> {
        if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::TrackLayout { track_count }) {
            tracing::warn!("[onion-skin] 轨布局未送达（降级补发接管）");
            return Vec::new();
        }

        let data = &self.root.editor.editor_state.data;
        let mut sent = Vec::new();
        for track_id in old_count..track_count {
            let parts = build_track_instance_parts(data, track_id);
            if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::TrackDelta { track_id, parts })
            {
                tracing::warn!("[onion-skin] 新增轨内容未送达（降级补发接管）");
                break;
            }
            sent.push(track_id);
        }

        tracing::debug!(
            "[onion-skin] 轨布局同步 {} → {} 段（新增轨内容 {:?}）",
            old_count,
            track_count,
            sent
        );
        sent
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
            if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::TrackDelta { track_id, parts })
            {
                tracing::warn!("[onion-skin] 事件级增量未送达（降级补发接管）");
                break;
            }
        }

        tracing::debug!(
            "[onion-skin] 事件级增量：更新 {} 个音轨 {:?} (track_gen={})",
            tracks.len(),
            tracks,
            fp.track_gen
        );
    }
}

/// CPU 单轨会话计划（纯逻辑，可单测）：`Some((track_id, 音符数))`。
///
/// 返回 `None` 表示当前轨越界/无文档——此时会话不发任何 `Chunk`（段表保持空）。
/// 音符数可为 0：仍要发一个空块占位段，否则主轨事件级增量（Insert/UpdateMany）
/// 在段表里找不到段。
fn current_only_plan(
    document: Option<&MidiDocument>,
    current_track: usize,
) -> Option<(usize, usize)> {
    let document = document?;
    if current_track >= document.track_count() {
        return None;
    }
    Some((current_track, document.track_notes(current_track).len()))
}

/// 把单音轨音符分块发送（每块 ≤ [`STREAMING_CHUNK_SIZE`]，末尾强制 flush）。
///
/// 末尾无条件的 flush 保证**空轨也发一个空块**（段表占位）；块刚好填满时多发一个
/// 空块是渲染侧的无操作（同轨续写 0 实例），不改变段表/数据。
fn send_track_chunked(
    document: &MidiDocument,
    track_id: usize,
    wgpu_thread: &WgpuRenderThread,
    aborted: &Cell<bool>,
    total_counter: &Cell<usize>,
) {
    let doc_notes = document.track_notes(track_id);
    let color = lumino_extras::palette::current_track_color_f32(track_id);
    // 轨道索引编码进 border_width 高 16 位（统一编码：主音轨判定 + 稳定深度）
    let border_width = onion_border_width(track_id);
    let total = doc_notes.len();
    let mut chunk: Vec<NoteInstance> = Vec::with_capacity(total.min(STREAMING_CHUNK_SIZE));
    let mut pushed = 0usize;
    for ne in doc_notes.iter() {
        chunk.push(NoteInstance::new(
            ne.start_tick as f32,
            ne.key,
            (ne.end_tick - ne.start_tick) as f32,
            color,
            border_width,
        ));
        pushed += 1;
        if chunk.len() >= STREAMING_CHUNK_SIZE {
            flush_chunk(&mut chunk, track_id, wgpu_thread, aborted, total_counter);
            if aborted.get() {
                return;
            }
            // 显式预留下一块（按剩余音符数，至多一块）：`flush_chunk` 取走后容量归零，
            // 不预留会让超大单轨每块都从 0 倍增重分配（30M 音符 ≈ 数百 MB 额外 memcpy）。
            chunk.reserve(total.saturating_sub(pushed).min(STREAMING_CHUNK_SIZE));
        }
    }
    flush_chunk(&mut chunk, track_id, wgpu_thread, aborted, total_counter);
}

/// 发送一个数据块（`chunk` 被清空复用；未送达 → 置会话中止标志）。
fn flush_chunk(
    chunk: &mut Vec<NoteInstance>,
    track_id: usize,
    wgpu_thread: &WgpuRenderThread,
    aborted: &Cell<bool>,
    total_counter: &Cell<usize>,
) {
    if aborted.get() {
        return;
    }
    let instances = std::mem::take(chunk);
    total_counter.set(total_counter.get() + instances.len());
    if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::Chunk {
        track_id,
        instances,
    }) {
        aborted.set(true);
    }
}

/// CPU 单轨会话计划测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "stream/tests.rs"]
mod tests;
