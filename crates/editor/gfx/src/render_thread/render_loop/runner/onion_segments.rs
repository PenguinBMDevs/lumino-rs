//! 洋葱皮 GPU 布局段表与事件级增量应用
//!
//! 黑乐谱场景（单音轨海量音符）的增量上传核心：
//! - 全量流式会话中，WGPU 侧按 `Chunk { track_id }` 流构建段表（track_id → offset/len）
//! - 编辑其他音轨（洋葱皮显示的音轨）时，UI 只发送 `TrackDelta { track_id, instances }`，
//!   本模块完成段替换：
//!   - 等长替换 → 仅 write_segment（音符级增量，只传该音轨）
//!   - 变长替换 → grow（如需）→ GPU 内部搬移后续段 → 写段 → 更新计数与段表
//!
//! 主音轨段内增量（2026-08-06 统一全量渲染）：GPU 布局 = 所有轨全部音符，
//! 主音轨编辑事件（`NoteEvent`，index = notes 索引）映射到「当前音轨段」的
//! 段内位置（`seg.offset + index`）：
//! - `UpdateMany` / `Update`：等长原位写（不改变段长）
//! - `RemoveAt` / `Insert`：段内删/插 → GPU 搬移 + 段表长度/偏移联动
//!
//! 正确性保障：
//! - `compute_move_blocks` 为纯函数，搬移分块序列已用 `Vec::copy_within`
//!   对照单测
//! - 段表偏移更新为纯计算，单测覆盖前插/删除/尾部增删/无后续段等边界

use crate::NoteEvent;
use crate::NoteInstance;
use crate::render_thread::render_loop::Renderers;

/// 洋葱皮 GPU 缓冲区中的音轨段
///
/// 布局：所有洋葱皮音轨按全量会话到达顺序紧凑排列在单 buffer 中，
/// 段间无间隙。`offset` = 首实例索引，`len` = 实例数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnionSegment {
    pub track_id: usize,
    pub offset: usize,
    pub len: usize,
}

/// 主音轨事件级增量：将 `NoteEvent` 应用到当前音轨段（段内位置 = `seg.offset + index`）
///
/// 返回 `true` 表示有事件被消费（调用方据此决定是否需要更新 cull info）。
/// 段不存在（如全量会话尚未到达 / 切轨瞬间事件错位）→ 防御性忽略并记录警告；
/// 下次全量会话（加载/布局变化）会重建段表兜底。
///
/// 事件语义（统一全量渲染后）：
/// - `UpdateMany` / `Update`：index = 当前轨 document notes 索引（保序）→ 段内原位写
/// - `RemoveAt` / `Remove`：段内保序删除，段长与后续段偏移联动
/// - `Insert` / `Add`：段内保序插入，段长与后续段偏移联动
/// - `Reset` / `Clear`：整段替换 / 清空（防御性兜底，正常路径 UI 不再发送）
pub fn process_main_track_events(
    renderers: &mut Renderers,
    segments: &mut [OnionSegment],
    current_track_encoded: u32,
    rx: &std::sync::mpsc::Receiver<NoteEvent>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> bool {
    if current_track_encoded == 0 {
        drain_stale_events(rx, None);
        return false;
    }
    let track_id = current_track_encoded as usize - 1;
    let Some(idx) = segments.iter().position(|s| s.track_id == track_id) else {
        // 段表缺该轨（如新增轨尚未收到 TrackLayout）：**丢弃**本批事件。
        // 事件内容以 document 为准，会由 TrackLayout + 新轨 TrackDelta 补齐；
        // 若堆积到下次全量会话后应用，会在已含该编辑的新段上二次应用（脏数据）。
        drain_stale_events(rx, Some(track_id));
        return false;
    };

    let mut updated = false;
    while let Ok(event) = rx.try_recv() {
        updated = true;
        // 每轮重新读取段元数据：段长可能被前序事件（删/插）改变
        let seg = segments[idx];
        match event {
            NoteEvent::UpdateMany {
                start_index,
                instances,
            } => {
                // 等长原位写：不改变段长，无需段表联动
                renderers
                    .onion_skin
                    .update_notes(seg.offset + start_index, &instances);
            }
            NoteEvent::Update { index, instance } => {
                renderers
                    .onion_skin
                    .update_notes(seg.offset + index, &[instance]);
            }
            NoteEvent::RemoveAt { index, count } => {
                let count = count.min(seg.len.saturating_sub(index));
                if count == 0 {
                    continue;
                }
                renderers.onion_skin.remove_at(seg.offset + index, count);
                shift_segment_len(renderers, segments, idx, -(count as isize), device, queue);
            }
            NoteEvent::Remove(index) => {
                if index >= seg.len {
                    continue;
                }
                renderers.onion_skin.remove_at(seg.offset + index, 1);
                shift_segment_len(renderers, segments, idx, -1, device, queue);
            }
            NoteEvent::Insert { index, instances } => {
                let index = index.min(seg.len);
                if instances.is_empty() {
                    continue;
                }
                renderers
                    .onion_skin
                    .insert_at(seg.offset + index, &instances);
                shift_segment_len(
                    renderers,
                    segments,
                    idx,
                    instances.len() as isize,
                    device,
                    queue,
                );
            }
            NoteEvent::Add(instance) => {
                renderers
                    .onion_skin
                    .insert_at(seg.offset + seg.len, &[instance]);
                shift_segment_len(renderers, segments, idx, 1, device, queue);
            }
            NoteEvent::Reset(instances) => {
                // 防御性兜底：整段替换（正常路径 UI 不再发送 Reset）
                apply_onion_track_delta(renderers, segments, track_id, &[instances], device, queue);
            }
            NoteEvent::Clear => {
                apply_onion_track_delta(renderers, segments, track_id, &[], device, queue);
            }
        }
    }
    updated
}

/// 段表缺少目标轨时丢弃通道内残留事件（以 document 为权威源重建，防二次应用）
fn drain_stale_events(rx: &std::sync::mpsc::Receiver<NoteEvent>, track_id: Option<usize>) -> usize {
    let mut dropped = 0usize;
    while rx.try_recv().is_ok() {
        dropped += 1;
    }
    if dropped > 0 {
        tracing::warn!(
            "MainTrack: 段表缺少 track={:?} 的段，丢弃 {} 条主轨事件（以 document 为准，待 TrackLayout/TrackDelta 同步）",
            track_id,
            dropped
        );
    }
    dropped
}

/// 段长变化后的段表联动：更新本段长度 + 后续段偏移平移，并刷新 cull info
fn shift_segment_len(
    renderers: &mut Renderers,
    segments: &mut [OnionSegment],
    idx: usize,
    delta: isize,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    segments[idx].len = (segments[idx].len as isize + delta) as usize;
    for seg in &mut segments[idx + 1..] {
        seg.offset = (seg.offset as isize + delta) as usize;
    }
    renderers.onion_skin.update_cull_info(device, queue);
}

/// 应用单音轨增量替换：将 `track_id` 段整体替换为 `instances`
///
/// 返回 `true` 表示成功；段表中无该音轨（UI/WGPU 状态不一致）返回 `false`，
/// 调用方记录警告并跳过——下次全量会话（音轨进出洋葱皮 / mute / 调色板变化）
/// 会重建段表兜底。
///
/// 等长（new_len == old_len）：
///     write_segment 原位覆盖（cull uniform 无需更新，bind group 有效）
/// 变长（new_len != old_len）：
///     1. 若新总实例数超容量 → grow（重建 buffer，GPU 内部复制现有数据）
///     2. GPU 内部搬移后续段（[old_end, old_count) → [new_end, new_count)，
///        staging 分块，见 `GpuNoteBuffer::move_range`）
///     3. 写新段 → 更新计数 → 更新段表偏移 → update_cull_info（count 变化
///        会重建 bind group）
pub fn apply_onion_track_delta(
    renderers: &mut Renderers,
    segments: &mut [OnionSegment],
    track_id: usize,
    parts: &[Vec<NoteInstance>],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> bool {
    let Some(idx) = segments.iter().position(|s| s.track_id == track_id) else {
        tracing::warn!(
            "OnionSkin: TrackDelta 的 track_id={} 不在段表中（状态不一致），跳过；等待下次全量会话修复",
            track_id
        );
        return false;
    };

    let old_len = segments[idx].len;
    let new_len: usize = parts.iter().map(Vec::len).sum();
    // 分片按序写入段内连续区间（大轨并行构建 → 免二次拼接拷贝）
    let write_parts = |renderers: &mut Renderers, base: usize| {
        let mut offset = base;
        for part in parts {
            renderers.onion_skin.write_segment(offset, part);
            offset += part.len();
        }
    };
    if old_len == new_len {
        // 等长替换：音符级增量，无需动段表 / cull info
        write_parts(renderers, segments[idx].offset);
        // 常驻字节变更 → 换代（全局桶重建）。
        renderers.onion_epoch = renderers.onion_epoch.wrapping_add(1);
        return true;
    }

    let old_count = renderers.onion_skin.gpu_instance_count();
    let delta = new_len as isize - old_len as isize;
    let new_count = (old_count as isize + delta) as usize;

    // 1. 扩容（grow 复制旧数据 → 之后的搬移/写段都在新 buffer 上）
    if new_count > renderers.onion_skin.gpu_capacity() && !renderers.onion_skin.grow_gpu(new_count)
    {
        tracing::error!(
            "OnionSkin: TrackDelta grow 失败（track={}, 需要容量 {}），跳过增量",
            track_id,
            new_count
        );
        return false;
    }

    // 2. 搬移后续段（GPU 内部，无 CPU 镜像）
    let tail_start = segments[idx].offset + old_len;
    let tail_count = old_count.saturating_sub(tail_start);
    if tail_count > 0 {
        renderers.onion_skin.move_gpu_range(
            tail_start,
            (tail_start as isize + delta) as usize,
            tail_count,
        );
    }

    // 3. 写新段（目标区间 = [offset, offset + new_len)，与搬移后的后续段相邻不重叠）
    write_parts(renderers, segments[idx].offset);

    // 4. 更新计数与段表
    renderers.onion_skin.set_gpu_instance_count(new_count);
    segments[idx].len = new_len;
    for seg in &mut segments[idx + 1..] {
        seg.offset = (seg.offset as isize + delta) as usize;
    }

    // 5. cull info（count 变化 → 重建 bind group；did_grow 时 buffer 句柄变化也会重建）
    renderers.onion_skin.update_cull_info(device, queue);

    // 常驻字节变更（搬移/写段）→ 换代（全局桶重建）。
    renderers.onion_epoch = renderers.onion_epoch.wrapping_add(1);

    tracing::debug!(
        "OnionSkin: TrackDelta track={} {} 实例 (原 {} 实例，{}) → 增量完成",
        track_id,
        new_len,
        old_len,
        if delta == 0 {
            "等长替换"
        } else {
            "变长搬移"
        }
    );
    true
}

/// 应用文档轨数变化：增量增删段表（**不重建既有轨**）。
///
/// - 增长（新增音轨）：追加零长段（offset = 现有实例数末端），无 GPU 数据搬移；
///   新轨内容由 UI 随后的 `TrackDelta` 补齐。
/// - 缩短（删除音轨）：尾部段的实例区间一次性 GPU 删除 + 截断段表 + cull 刷新。
pub fn apply_onion_track_layout(
    renderers: &mut Renderers,
    segments: &mut Vec<OnionSegment>,
    track_count: usize,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    if segments.len() == track_count {
        return;
    }
    let (new_segments, removed) = layout_segments_after_track_count(segments, track_count);
    if let Some((start, count)) = removed {
        renderers.onion_skin.remove_at(start, count);
        renderers.onion_epoch = renderers.onion_epoch.wrapping_add(1);
        renderers.onion_skin.update_cull_info(device, queue);
    }
    tracing::debug!(
        "OnionSkin: 轨布局同步 {} → {} 段（删除实例区间 {:?}）",
        segments.len(),
        new_segments.len(),
        removed
    );
    *segments = new_segments;
}

/// 计算轨布局变更后的段表（纯函数，可单测）。
///
/// 返回 `(new_segments, removed_range)`：
/// - 增长：追加零长段，`removed_range = None`
/// - 缩短：截断尾部段，`removed_range = Some((start, count))` 为需从 GPU
///   缓冲区删除的实例区间（尾部段紧凑排列，恒在缓冲区末端）
pub(crate) fn layout_segments_after_track_count(
    segments: &[OnionSegment],
    track_count: usize,
) -> (Vec<OnionSegment>, Option<(usize, usize)>) {
    if track_count >= segments.len() {
        let base = segments.last().map(|s| s.offset + s.len).unwrap_or(0);
        let mut out = segments.to_vec();
        for track_id in segments.len()..track_count {
            out.push(OnionSegment {
                track_id,
                offset: base,
                len: 0,
            });
        }
        return (out, None);
    }
    let start = segments[track_count].offset;
    let removed: usize = segments[track_count..].iter().map(|s| s.len).sum();
    let removed = (removed > 0).then_some((start, removed));
    (segments[..track_count].to_vec(), removed)
}

/// 应用单音轨区间级删除：将 `track_id` 段内的若干区间删除（批量删除增量路径）。
///
/// `ranges` 为 `(index, count)`，语义与 `NoteEvent::RemoveAt` 一致，**必须降序**：
/// 高索引先删，低索引不漂移；各区间针对删除前的段内容。整轨只做
/// 「区间数次 GPU 尾部搬移 + 一次 cull 刷新」，免整轨实例重建与重传。
///
/// 返回 `true` 表示成功；段表中无该音轨返回 `false`（调用方记录警告并跳过）。
pub fn apply_onion_track_remove_ranges(
    renderers: &mut Renderers,
    segments: &mut [OnionSegment],
    track_id: usize,
    ranges: &[(usize, usize)],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> bool {
    let Some(idx) = segments.iter().position(|s| s.track_id == track_id) else {
        tracing::warn!(
            "OnionSkin: TrackRemoveRanges 的 track_id={} 不在段表中（状态不一致），跳过",
            track_id
        );
        return false;
    };

    let offset = segments[idx].offset;
    let mut removed_total = 0usize;
    for &(index, count) in ranges {
        // 当前段长已扣除更高区间 → 直接以 index 相对当前段定位
        let count = count.min(segments[idx].len.saturating_sub(index));
        if count == 0 {
            continue;
        }
        renderers.onion_skin.remove_at(offset + index, count);
        segments[idx].len -= count;
        removed_total += count;
    }
    if removed_total == 0 {
        return true;
    }
    for seg in &mut segments[idx + 1..] {
        seg.offset -= removed_total;
    }
    renderers.onion_skin.update_cull_info(device, queue);
    renderers.onion_epoch = renderers.onion_epoch.wrapping_add(1);
    tracing::debug!(
        "OnionSkin: TrackRemoveRanges track={} 删除 {} 实例（{} 个区间）",
        track_id,
        removed_total,
        ranges.len()
    );
    true
}

/// 段表纯函数测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "onion_segments/tests.rs"]
mod tests;
