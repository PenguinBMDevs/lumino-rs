//! 主音轨事件级增量（Phase 0）：拖动 / 变速 / 翻转 / ghost 的局部更新
//!
//! 卷帘编辑增量（2026-08-05）：GPU 布局 = 上次全量构建的可见音符
//! （`note_visible_indices` 列表，GPU 位置 = 列表下标）。本模块尝试用
//! 事件/ghost 位置做局部 `UpdateMany` 更新，命中即跳过每帧全量构建上传。
//!
//! 两条路径（`try_note_incremental_update`，命中返回 true）：
//! 1. **数据事件**：拖动 release / 变速 / 翻转 / 异步提交（editor-state 数据层
//!    已记录 `NoteDeltaEvent`），映射到 GPU 段后增量发送
//! 2. **ghost 拖动**：拖动中 `data.notes` 未变（ghost 方案），只更新被拖动
//!    音符的渲染位置（`Editor::build_ghost_delta_positions`）
//!
//! 兜底：事件含不可见索引（如全选变速）或 dirty（undo/加载/切轨/散改）→
//! 返回 false，调用方走全量重建（正确性无损：全量使用最新 notes 数据）。

use crate::host::Host;
use crate::host::render::note_delta;
use crate::host::render::note_worker;
use lumino_gfx::{NoteEvent, NoteInstance};

/// 主音轨音符描边：固定 1 像素（用户要求，与全量构建一致）
const MAIN_TRACK_BORDER_WIDTH: u32 = 1;

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
                    // 未知变化（undo/加载/切轨/散改）→ 清队列，走全量兜底
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
                    for (start, instances) in segments {
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: start,
                            instances,
                        });
                    }
                    tracing::trace!(
                        "[note-delta] 事件级增量：{} 事件，跳过全量构建",
                        events.len()
                    );
                    return true;
                }
                // 事件含不可见索引（如全选变速）→ 落全量兜底
                // （事件已消费，全量构建使用最新 notes 数据，正确性无损）
                tracing::debug!(
                    "[note-delta] 事件含不可见索引，回退全量重建（{} 事件）",
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
                    for (next, instances) in segments {
                        let start = next - instances.len();
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: start,
                            instances,
                        });
                    }
                    tracing::trace!("[note-delta] ghost 拖动增量：{} 段，跳过全量构建", 0);
                    return true;
                }
            }
        }

        false
    }
}
