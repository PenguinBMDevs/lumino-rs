//! 主音轨事件级增量（Phase 0）：拖动 / 变速 / 翻转 / ghost / 可见列表 diff
//!
//! 卷帘编辑增量（2026-08-05）：GPU 布局 = 上次全量构建的可见音符
//! （`note_visible_indices` 列表，GPU 位置 = 列表下标）。本模块尝试用
//! 事件/ghost 位置做局部 `UpdateMany` 更新，命中即跳过每帧全量构建上传。
//!
//! 三条路径（`try_note_incremental_update`，命中返回 true）：
//! 1. **数据事件**：拖动 release / 变速 / 翻转 / 异步提交（editor-state 数据层
//!    已记录 `NoteDeltaEvent`），映射到 GPU 段后增量发送
//! 2. **ghost 拖动**：拖动中 `data.notes` 未变（ghost 方案），只更新被拖动
//!    音符的渲染位置（`Editor::build_ghost_delta_positions`）
//! 3. **可见列表 diff**（2026-08-06）：切轨 / 增删 / undo/redo / 散改 / 事件含
//!    不可见索引等无法用事件队列精确描述的兜底路径，收集新可见列表与
//!    「上次 GPU 布局镜像」做前缀/后缀对齐 diff，只传输差异段——
//!    切轨到相同内容/空轨零上传，任何编辑过程不再全量重传。
//!
//! 镜像同步纪律：GPU 布局镜像 `main_note_instances` 是 diff 路径的基准，
//! 全量构建（runner.rs Phase 1）与三条增量路径都必须同步它，保证
//! 「镜像 == GPU 内容」不变式。

use crate::host::Host;
use crate::host::render::note_delta;
use crate::host::render::note_diff::{DiffResult, diff_visible};
use crate::host::render::note_worker;
use lumino_gfx::{NoteEvent, NoteInstance};

/// 主音轨音符描边：固定 1 像素（用户要求，与全量构建一致）
const MAIN_TRACK_BORDER_WIDTH: u32 = 1;

/// 可见列表收集 overscan 因子（与 runner.rs 全量路径一致）
const OVERSCAN_FACTOR: f32 = 0.5;

/// 从 `NoteInstance` 还原可见三元组 `(tick, key, length)`（镜像同步用）
#[inline]
fn instance_to_triple(inst: &NoteInstance) -> (f32, u16, f32) {
    (
        inst.start_length[0],
        (inst.key_color & 0xFF) as u16,
        inst.start_length[1],
    )
}

/// 将 GPU 段（UpdateMany）应用到布局镜像（段内逐个覆盖写）
fn mirror_apply_updates(mirror: &mut [(f32, u16, f32)], segments: &[(usize, Vec<NoteInstance>)]) {
    for (start, instances) in segments {
        for (i, inst) in instances.iter().enumerate() {
            let idx = start + i;
            if idx < mirror.len() {
                mirror[idx] = instance_to_triple(inst);
            }
        }
    }
}

/// 构建主音轨普通音符实例（与全量构建颜色/描边一致）
#[inline]
fn build_instance(t: f32, k: u16, l: f32) -> NoteInstance {
    NoteInstance::new(
        t,
        k as u8,
        l,
        note_worker::MAIN_TRACK_NOTE_COLOR,
        MAIN_TRACK_BORDER_WIDTH,
    )
}

impl Host {
    /// 尝试主音轨事件级增量更新，返回 `true` = 已增量处理（调用方跳过全量构建）
    ///
    /// 增量前提（全部满足才尝试）：
    /// - 数据变化（`note_index_dirty` 或 `is_ghost_dragging`）且视口未变
    ///   （可见列表与 GPU 布局一致）
    /// - 无预览音符（Drawing / hover 预览走全量，含预览实例构建）
    pub(super) fn try_note_incremental_update(
        &mut self,
        note_index_dirty: bool,
        viewport_changed: bool,
        is_drawing: bool,
        is_hover_preview: bool,
        is_ghost_dragging: bool,
    ) -> bool {
        // ── 路径 1：数据事件增量（拖动 release / 变速 / 翻转 / 异步提交）──
        if note_index_dirty && !viewport_changed && !is_drawing && !is_hover_preview {
            let events = {
                let data = &mut self.root.editor.editor_state.data;
                if data.note_delta_dirty {
                    // 未知变化（undo/加载/切轨/散改）→ 清队列，走路径 3 diff 兜底
                    data.note_delta_events.clear();
                    data.note_delta_dirty = false;
                    None
                } else {
                    let e = data.take_note_delta_events();
                    if e.is_empty() { None } else { Some(e) }
                }
            };
            if let Some(events) = events {
                let visible = &self.render_ctx.render_cache.note_visible_indices;
                let color = note_worker::MAIN_TRACK_NOTE_COLOR;
                let mapped = note_delta::map_events_to_segments(&events, visible, |note| {
                    NoteInstance::new(
                        note.tick,
                        note.key as u8,
                        note.length,
                        color,
                        MAIN_TRACK_BORDER_WIDTH,
                    )
                });
                if let Ok(segments) = mapped {
                    for (start, instances) in &segments {
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: *start,
                            instances: instances.clone(),
                        });
                    }
                    // 镜像同步：GPU 段已更新，布局镜像同步覆盖写
                    mirror_apply_updates(
                        &mut self.render_ctx.render_cache.main_note_instances,
                        &segments,
                    );
                    // 数据代际已变化（事件记录时 bump），同步缓存避免路径 3 重复 diff
                    self.render_ctx.render_cache.last_note_gen =
                        self.root.editor.editor_state.data.track_notes_gen;
                    self.render_ctx.render_cache.last_built_track =
                        self.root.editor.editor_state.data.current_track;
                    tracing::trace!(
                        "[note-delta] 事件级增量：{} 事件，跳过全量构建",
                        events.len()
                    );
                    return true;
                }
                // 事件含不可见索引（如全选变速）→ 落路径 3 diff 兜底
                // （事件已消费，diff 使用最新 notes 数据，正确性无损）
                tracing::debug!(
                    "[note-delta] 事件含不可见索引，回退可见列表 diff（{} 事件）",
                    events.len()
                );
            }
        }

        // ── 路径 2：ghost 拖动增量（拖动中 notes 未变，只更新被拖动音符）──
        if is_ghost_dragging
            && !viewport_changed
            && !is_drawing
            && !is_hover_preview
            && !self.render_ctx.render_cache.note_instances_is_empty()
        {
            // clone 可见索引避免与 editor 借用冲突（拖动帧低频，可接受）
            let visible = self.render_ctx.render_cache.note_visible_indices.clone();
            if !visible.is_empty() {
                let positions = self.root.editor.build_ghost_delta_positions(&visible);
                if !positions.is_empty() {
                    let color = note_worker::MAIN_TRACK_NOTE_COLOR;
                    // 合并连续 GPU 段（段元组 (下一个位置, 实例列表)）
                    let mut segments: Vec<(usize, Vec<NoteInstance>)> = Vec::new();
                    for (pos, (tick, key, length)) in positions {
                        let instance = NoteInstance::new(
                            tick,
                            key as u8,
                            length,
                            color,
                            MAIN_TRACK_BORDER_WIDTH,
                        );
                        match segments.last_mut() {
                            Some((next, instances)) if *next == pos => {
                                instances.push(instance);
                                *next = pos + 1;
                            }
                            _ => segments.push((pos + 1, vec![instance])),
                        }
                    }
                    for (next, instances) in &segments {
                        let start = next - instances.len();
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: start,
                            instances: instances.clone(),
                        });
                    }
                    // 镜像同步：ghost 位置变化同样覆盖镜像（拖动中 document 未变）
                    mirror_apply_updates(
                        &mut self.render_ctx.render_cache.main_note_instances,
                        &segments,
                    );
                    tracing::trace!("[note-delta] ghost 拖动增量：{} 段，跳过全量构建", 0);
                    return true;
                }
            }
        }

        // ── 路径 3：可见列表 diff 增量（切轨/增删/undo/散改/事件不可见兜底）──
        //
        // 数据代际（track_notes_gen）或音轨变化时，收集新可见列表与 GPU 布局
        // 镜像做前缀/后缀对齐 diff：全等零上传（切轨到相同内容/空轨不再全量
        // 重传），局部差异只传 UpdateMany/RemoveAt/Insert 段，差异过大才 Reset
        // 一次全量写（内容全变时比多段搬移+写更便宜）。
        if !viewport_changed && !is_drawing && !is_hover_preview {
            let changed = {
                let data = &self.root.editor.editor_state.data;
                data.track_notes_gen != self.render_ctx.render_cache.last_note_gen
                    || data.current_track != self.render_ctx.render_cache.last_built_track
                    // 索引脏兜底：任何「只标记脏未 bump 代际」的变化都会被
                    // collect + diff 与镜像对账捕获（数据确实没变 → Noop 零上传）
                    || note_index_dirty
            };
            if changed && !self.render_ctx.render_cache.main_note_instances.is_empty() {
                // Phase A：收集新可见列表 + 计算 diff（借用拆分：镜像只读 + buffer 可写）
                let (new_visible, diff) = {
                    let cache = &mut self.render_ctx.render_cache;
                    self.root.editor.collect_visible_note_data(
                        &mut cache.visible_notes_buffer,
                        Some(&mut cache.note_visible_indices),
                        OVERSCAN_FACTOR,
                    );
                    let new_visible = std::mem::take(&mut cache.visible_notes_buffer);
                    let diff = diff_visible(&cache.main_note_instances, &new_visible);
                    (new_visible, diff)
                };

                // Phase B：按 diff 结果发送增量事件（顺序：updates → removes → inserts）
                match diff {
                    DiffResult::Noop => {
                        tracing::trace!("[note-delta] 可见列表 diff：内容一致（切轨/编辑零上传）");
                    }
                    DiffResult::Full => {
                        let instances: Vec<NoteInstance> = new_visible
                            .iter()
                            .map(|&(t, k, l)| build_instance(t, k, l))
                            .collect();
                        self.send_note_event_to_render_thread(NoteEvent::Reset(instances));
                        tracing::debug!(
                            "[note-delta] 可见列表 diff：差异过大，Reset 全量写（{} 实例）",
                            new_visible.len()
                        );
                    }
                    DiffResult::Segments(d) => {
                        for (start, triples) in &d.updates {
                            let instances: Vec<NoteInstance> = triples
                                .iter()
                                .map(|&(t, k, l)| build_instance(t, k, l))
                                .collect();
                            self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                                start_index: *start,
                                instances,
                            });
                        }
                        for (index, count) in &d.removes {
                            self.send_note_event_to_render_thread(NoteEvent::RemoveAt {
                                index: *index,
                                count: *count,
                            });
                        }
                        for (index, triples) in &d.inserts {
                            let instances: Vec<NoteInstance> = triples
                                .iter()
                                .map(|&(t, k, l)| build_instance(t, k, l))
                                .collect();
                            self.send_note_event_to_render_thread(NoteEvent::Insert {
                                index: *index,
                                instances,
                            });
                        }
                        tracing::trace!(
                            "[note-delta] 可见列表 diff：{} 更新段 / {} 删除段 / {} 插入段，跳过全量构建",
                            d.updates.len(),
                            d.removes.len(),
                            d.inserts.len()
                        );
                    }
                }

                // Phase C：同步缓存（镜像 = 新列表 + 数据代际/音轨），保持
                // 「镜像 == GPU 内容」不变式
                let cache = &mut self.render_ctx.render_cache;
                cache.main_note_instances = new_visible;
                cache.last_note_gen = self.root.editor.editor_state.data.track_notes_gen;
                cache.last_built_track = self.root.editor.editor_state.data.current_track;
                self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;
                return true;
            }
        }

        false
    }
}
