//! 音符数据更新 — 统一全量渲染：事件段内增量 + 预览
//!
//! 包含 Host 的以下方法：
//! - `update_note_data_for_wgpu_thread`: 更新音符数据
//! - `build_preview_instances`: 构建预览音符实例

use crate::host::Host;
use crate::host::render::note_worker;
use lumino_gfx::{NoteEvent, NoteInstance, OnionSkinStreamMsg};

/// 主音轨音符描边：固定 1 像素（用户要求）
const MAIN_TRACK_BORDER_WIDTH: u32 = 1;

/// 编码当前音轨实例的 border_width：低 16 位 = 固定 1 像素描边，
/// 高 16 位 = track_idx + 1（统一全量渲染中 VS 据此判断主音轨并输出稳定深度）。
fn main_track_border_width(track_idx: usize) -> u32 {
    MAIN_TRACK_BORDER_WIDTH | (((track_idx as u32) + 1) << 16)
}

/// ghost 拖动可见索引收集的 overscan 因子（与历史可见收集一致）
const GHOST_OVERSCAN: f32 = 0.5;

impl Host {
    /// 更新 WGPU 渲染线程的音符数据（统一全量渲染，2026-08-06）
    ///
    /// GPU buffer 常驻**所有轨全部音符**（洋葱皮全量会话 + 段表），主音轨 =
    /// 当前音轨段（ViewState uniform 着色，切轨零重传）。本函数只负责：
    /// 1. 未知变化兜底（undo/加载/散改）→ 强制全量会话分块重建（CPU 峰值可控）
    /// 2. 编辑事件 → 段内 UpdateMany（index = notes 索引，GPU 布局 = 全量轨段）
    /// 3. ghost 拖动 / 复制副本 → 段内 UpdateMany / 预览通道
    /// 4. 预览音符（Drawing/hover/i2m）→ 独立预览渲染器
    ///
    /// 滚动/缩放/切轨**零重传**：视口变化只更新 camera uniform（渲染线程
    /// prepare_pass），GPU cull 每帧剔除；切轨只发 SetViewState（onion_skin
    /// 决策层检测 current_track 变化）。
    pub(crate) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");

        // 走带模式使用 arrangement_renderer，不需要音符实例
        if self.root.is_arrangement_mode() {
            return;
        }

        // ── 1. 未知变化兜底：undo/redo/加载/散改等无事件可对账的变化
        // → 强制全量会话重建（洋葱皮分块流式；段表重建后主音轨段 = 最新 document）
        if self.root.editor.editor_state.data.note_delta_dirty {
            let data = &mut self.root.editor.editor_state.data;
            data.note_delta_events.clear();
            data.note_delta_dirty = false;
            self.render_ctx.onion_skin_state.force_full_next();
        }

        // ── 2. 主音轨事件级增量（段内）：index = notes 索引（保序，
        // GPU 段内位置 = 段 offset + index，由渲染线程按当前音轨段应用）
        let events = self.root.editor.editor_state.data.take_note_delta_events();
        if !events.is_empty() {
            let current_track = self.root.editor.editor_state.data.current_track;
            let color = lumino_extras::palette::current_track_color_f32(current_track);
            let border_width = main_track_border_width(current_track);
            // 合并连续 UpdateRange；遇到 Insert/Remove 时先 flush 当前 UpdateRange
            let mut update_segments: Vec<(usize, Vec<NoteInstance>)> = Vec::new();
            let flush_update = |segments: &mut Vec<(usize, Vec<NoteInstance>)>| {
                for (next, instances) in segments.drain(..) {
                    if !instances.is_empty() {
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: next - instances.len(),
                            instances,
                        });
                    }
                }
            };
            for event in &events {
                match event {
                    lumino_editor_state::NoteDeltaEvent::UpdateRange { start_index, notes } => {
                        for (offset, note) in notes.iter().enumerate() {
                            let idx = start_index + offset;
                            let instance = NoteInstance::new(
                                note.tick,
                                note.key as u8,
                                note.length,
                                color,
                                border_width,
                            );
                            match update_segments.last_mut() {
                                Some((next, insts)) if *next == idx => {
                                    insts.push(instance);
                                    *next = idx + 1;
                                }
                                _ => update_segments.push((idx + 1, vec![instance])),
                            }
                        }
                    }
                    lumino_editor_state::NoteDeltaEvent::InsertAt { index, note } => {
                        flush_update(&mut update_segments);
                        let instance = NoteInstance::new(
                            note.tick,
                            note.key as u8,
                            note.length,
                            color,
                            border_width,
                        );
                        self.send_note_event_to_render_thread(NoteEvent::Insert {
                            index: *index,
                            instances: vec![instance],
                        });
                    }
                    lumino_editor_state::NoteDeltaEvent::RemoveAt { index, count } => {
                        flush_update(&mut update_segments);
                        self.send_note_event_to_render_thread(NoteEvent::RemoveAt {
                            index: *index,
                            count: *count,
                        });
                    }
                }
            }
            flush_update(&mut update_segments);
            tracing::trace!(
                "[note-delta] 段内增量：{} 事件（GPU 布局 = 全量轨段）",
                events.len()
            );
        }

        // ── 3. ghost 拖动 / 复制副本（拖动中 document 未变，只更新被拖音符）──
        let mut preview_instances = Vec::new();
        if self.root.editor.has_active_ghost_delta_state() {
            let editor = &self.root.editor;
            // 视口内 notes 索引收集（仅索引，O(视口内)；ghost 拖动帧低频）
            let mut scratch: Vec<(f32, u16, f32)> = Vec::new();
            let mut indices: Vec<usize> = Vec::new();
            editor.collect_visible_note_data(&mut scratch, Some(&mut indices), GHOST_OVERSCAN);

            let copy_active = editor.has_pending_copy_drag();
            let current_track = self.root.editor.editor_state.data.current_track;
            let track_color = lumino_extras::palette::current_track_color_f32(current_track);
            let track_border_width = main_track_border_width(current_track);

            if copy_active {
                // 复制副本 → 合并到预览列表（原件已在 GPU 段原位，副本叠加渲染）
                let copy_color = note_worker::MAIN_TRACK_NOTE_COLOR;
                let copies = editor.build_copy_ghost_positions(&indices);
                preview_instances.reserve(copies.len());
                for &(tick, key, length) in &copies {
                    preview_instances.push(NoteInstance::new(
                        tick,
                        key as u8,
                        length,
                        copy_color,
                        MAIN_TRACK_BORDER_WIDTH,
                    ));
                }
            } else {
                // 普通 ghost 拖动 → 段内 UpdateMany（index = notes 索引）
                let positions = editor.build_ghost_delta_positions(&indices);
                if !positions.is_empty() {
                    // 合并连续段（段元组 (下一个位置, 实例列表)）
                    let mut segments: Vec<(usize, Vec<NoteInstance>)> = Vec::new();
                    for (idx, (tick, key, length)) in positions {
                        let instance = NoteInstance::new(
                            tick,
                            key as u8,
                            length,
                            track_color,
                            track_border_width,
                        );
                        match segments.last_mut() {
                            Some((next, insts)) if *next == idx => {
                                insts.push(instance);
                                *next = idx + 1;
                            }
                            _ => segments.push((idx + 1, vec![instance])),
                        }
                    }
                    for (next, instances) in segments {
                        self.send_note_event_to_render_thread(NoteEvent::UpdateMany {
                            start_index: next - instances.len(),
                            instances,
                        });
                    }
                }
            }
        }

        // ── 4. 预览音符（Drawing / hover / i2m）→ 合并到同一预览列表
        preview_instances.extend(self.build_preview_instances());
        self.send_onion_skin_msg_to_render_thread(OnionSkinStreamMsg::PreviewInstances(
            preview_instances,
        ));

        // 更新光标位置缓存
        self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;
    }

    /// 构建预览音符实例（Drawing / hover / i2m 预览，统一全量渲染用）
    ///
    /// 预览音符不在 document 中、不进全量 buffer；变化时整体发送到独立
    /// 预览渲染器（`OnionSkinStreamMsg::PreviewInstances`）。无预览返回空 Vec。
    fn build_preview_instances(&self) -> Vec<NoteInstance> {
        let editor = &self.root.editor;
        let edit_state = &editor.editor_state.interaction.edit_state;
        let default_note_length = editor.editor_state.view.default_note_length;
        let snap_precision = editor.editor_state.view.snap_precision;
        let preview_default_length = editor
            .editor_state
            .view
            .last_note_length
            .unwrap_or(default_note_length);
        let color = note_worker::MAIN_TRACK_NOTE_COLOR;

        // 正在绘制的音符（Drawing 状态）— 预览音符用 new_preview（哨兵）
        if let crate::editor::EditState::Drawing {
            start_tick,
            key,
            current_tick,
        } = edit_state
        {
            let (tick, length) = if *current_tick > *start_tick {
                (*start_tick, *current_tick - *start_tick)
            } else if *current_tick < *start_tick {
                (*current_tick, *start_tick - *current_tick)
            } else {
                (*start_tick, preview_default_length)
            };
            return vec![NoteInstance::new_preview(
                tick,
                *key as u8,
                length.max(snap_precision),
                color,
            )];
        }

        // 图片转 MIDI 预览：主轨实色 + 其他轨洋葱皮颜色（非哨兵，与旧渲染一致）
        let i2m = &editor.editor_state.image_to_midi;
        if i2m.is_active() {
            let (main_preview, onion_preview) = note_worker::collect_i2m_preview_notes(editor);
            if main_preview.is_empty() && onion_preview.is_empty() {
                return Vec::new();
            }
            let mut out = Vec::with_capacity(main_preview.len() + onion_preview.len());
            for (tick, key, length) in main_preview {
                out.push(NoteInstance::new(
                    tick,
                    key,
                    length,
                    color,
                    MAIN_TRACK_BORDER_WIDTH,
                ));
            }
            for (tick, key, length, onion_color) in onion_preview {
                out.push(NoteInstance::new(
                    tick,
                    key,
                    length,
                    onion_color,
                    MAIN_TRACK_BORDER_WIDTH,
                ));
            }
            return out;
        }

        // hover 预览（铅笔工具 + Idle 状态，跟随鼠标指针）
        if matches!(edit_state, crate::editor::EditState::Idle)
            && editor.current_tool() == crate::message::Tool::Pencil
            && self.root.should_render_preview_note()
            && let Some((cx, cy)) = editor.editor_state.canvas.cursor_position
        {
            let view = &editor.editor_state.view;
            let canvas = &editor.editor_state.canvas;
            let local_x = cx - canvas.offset_x;
            let local_y = cy - canvas.offset_y;
            let in_canvas = local_x >= view.keyboard_width
                && local_y >= view.ruler_height
                && local_x < canvas.size_x
                && local_y < canvas.size_y;
            if in_canvas {
                let tick = view.snap_tick(view.x_to_tick(local_x)).max(0.0);
                let key = view.y_to_key(local_y);
                return vec![NoteInstance::new_preview(
                    tick,
                    key as u8,
                    preview_default_length.max(snap_precision),
                    color,
                )];
            }
        }

        Vec::new()
    }
}
