//! 拖动和调整大小操作

use super::{EditState, Editor};
use iced_core::Point;
use lumino_core::DragState;

impl Editor {
    /// 检查是否应从 PendingDrag 转换到 Dragging 状态
    pub(crate) fn try_transition_to_dragging(&mut self, pos: iced_core::Point) {
        let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.editor_state.interaction.edit_state
        else {
            return;
        };

        if !self.should_start_dragging(pos, Point::new(start_pos.0, start_pos.1)) {
            return;
        }

        // ghost 方案：拖动期间数据不动，仅维护 DragState 偏移
        let note_count = self.editor_state.data.notes.len();
        let drag_state = DragState::from_single(
            note_index,
            note_count,
            original_tick as i64,
            original_key as i16,
        );
        self.push_history();
        // 更新 editor_state
        self.editor_state.interaction.edit_state = EditState::Dragging {
            note_index,
            drag_state,
            last_played_key: original_key,
        };
    }

    /// 根据当前编辑状态计算音符的变化量
    ///
    /// 返回 (new_tick, new_key, new_length, note_to_play)
    ///
    /// **ghost 方案**：`Dragging` / `DraggingSelection` 期间不写入 `data.notes`，
    /// 仅更新 `DragState` 的 delta 偏移。渲染层用 `ghost_position` 实时计算预览位置。
    /// `new_tick` / `new_key` 仅用于 `ResizingStart` / `ResizingEnd`（这些仍走直接写入路径）。
    pub(crate) fn compute_state_changes(
        &mut self,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>, Option<u16>) {
        let v = &self.editor_state.view;
        let snap_precision = v.snap_precision;
        let visible_key_count = v.visible_key_count;
        let mut new_tick = None;
        let new_key = None;
        let mut new_length = None;
        let mut note_to_play = None;

        // 预读 Dragging 状态下音符原始位置（ghost 方案：drag 期间 data.notes 不变）
        let dragging_note_orig: Option<(f32, u16)> = match &self.editor_state.interaction.edit_state
        {
            EditState::Dragging { note_index, .. } => self
                .editor_state
                .data
                .notes
                .get(*note_index)
                .map(|n| (n.tick, n.key)),
            _ => None,
        };

        match &mut self.editor_state.interaction.edit_state {
            EditState::Selecting { .. } => {
                self.update_selection();
                return (None, None, None, None);
            }
            EditState::Drawing { current_tick, .. } => {
                *current_tick = snapped_tick;
            }
            EditState::Dragging {
                note_index: _,
                drag_state,
                last_played_key,
            } => {
                let Some((original_tick, original_key)) = dragging_note_orig else {
                    return (None, None, None, None);
                };
                // drag_state.initial_tick 是 mouse 拖动开始时的 tick
                let raw_delta_tick = tick - drag_state.initial_tick as f32;
                let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
                // calculated_key = key - (mouse_initial_key - original_key)
                //                = key - mouse_initial_key + original_key
                let calculated_key = (key as i32 - drag_state.initial_key as i32
                    + original_key as i32)
                    .clamp(0, visible_key_count.saturating_sub(1) as i32)
                    as u16;

                // 更新 drag_state 的 delta（用于 ghost 渲染与松手时 apply_to_notes）
                // delta_tick = snapped_delta_tick（音符偏移量）
                // delta_key = calculated_key - original_key（音符 key 偏移量）
                let _ = original_tick; // original_tick 用于 ghost_position 内部 clamp，此处无需再用
                let delta_key = (calculated_key as i16).saturating_sub(original_key as i16);
                drag_state.set_delta(snapped_delta_tick as i64, delta_key);

                if calculated_key != *last_played_key {
                    note_to_play = Some(calculated_key);
                    *last_played_key = calculated_key;
                }
                // ghost 方案：dragging 期间不写 notes，不返回 new_tick/new_key
            }
            EditState::ResizingStart {
                original_tick,
                original_length,
                ..
            } => {
                let end_tick = *original_tick + *original_length;
                let calculated_tick = snapped_tick.min(end_tick - snap_precision).max(0.0);
                new_tick = Some(calculated_tick);
                new_length = Some(end_tick - calculated_tick);
            }
            EditState::ResizingEnd { note_index, .. } => {
                if let Some(note) = self.editor_state.data.notes.get(*note_index) {
                    new_length = Some((snapped_tick - note.tick).max(snap_precision));
                }
            }
            EditState::DraggingSelection { drag_state } => {
                // ghost 方案：仅更新 drag_state 的 delta，不写 notes
                let raw_delta_tick = snapped_tick - drag_state.initial_tick as f32;
                let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
                let delta_tick_i = snapped_delta_tick as i64;
                let delta_key_i = (key as i32 - drag_state.initial_key as i32) as i16;

                if delta_tick_i != drag_state.delta_tick || delta_key_i != drag_state.delta_key {
                    drag_state.set_delta(delta_tick_i, delta_key_i);
                    // ghost 方案：data.notes 未变，仅 delta 变了。
                    // 用 mark_ghost_dirty 触发 wgpu 重绘，不触发空间索引重建。
                    // （误用 mark_notes_changed 会导致 3106 音符每帧重建 47ms × 60fps ≈ 2.8s 卡顿）
                    self.mark_ghost_dirty();
                }
            }
            EditState::ResizingSelectionStart { last_tick } => {
                let delta_tick = snapped_tick - *last_tick;

                if delta_tick != 0.0 {
                    let selected: Vec<usize> = self
                        .editor_state
                        .interaction
                        .selected_notes
                        .iter()
                        .copied()
                        .collect();

                    for i in selected {
                        if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                            let new_length = note.length - delta_tick;
                            if new_length >= snap_precision {
                                note.tick += delta_tick;
                                note.length = new_length;
                            }
                        }
                    }

                    *last_tick = snapped_tick;
                    // ghost 方案：Resizing 期间 notes 已改，但空间索引不每帧重建。
                    // 用 mark_ghost_dirty 只触发 wgpu 重绘（基于新 notes），不重建索引。
                    // 空间索引在松手时（released.rs）一次性 mark_notes_changed 重建。
                    // **性能关键**：1000W 音符建树 124ms，每帧重建 = 60fps × 124ms = 灾难。
                    self.mark_ghost_dirty();
                }
            }
            EditState::ResizingSelectionEnd { last_tick } => {
                let delta_tick = snapped_tick - *last_tick;

                if delta_tick != 0.0 {
                    let selected: Vec<usize> = self
                        .editor_state
                        .interaction
                        .selected_notes
                        .iter()
                        .copied()
                        .collect();

                    for i in selected {
                        if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                            let new_length = note.length + delta_tick;
                            if new_length >= snap_precision {
                                note.length = new_length;
                            }
                        }
                    }

                    *last_tick = snapped_tick;
                    // ghost 方案：同 ResizingSelectionStart，期间不重建索引
                    self.mark_ghost_dirty();
                }
            }
            _ => {}
        }

        (new_tick, new_key, new_length, note_to_play)
    }

    /// 更新框选区域中的音符选中状态
    pub(crate) fn update_selection(&mut self) {
        if let EditState::Selecting {
            start_tick,
            start_key,
            current_tick,
            current_key,
        } = self.editor_state.interaction.edit_state
        {
            let min_tick = start_tick.min(current_tick);
            let max_tick = start_tick.max(current_tick);
            let min_key = start_key.min(current_key);
            let max_key = start_key.max(current_key);

            self.editor_state.interaction.selected_notes.clear();
            for (i, note) in self.editor_state.data.notes.iter().enumerate() {
                let note_end = note.tick + note.length;

                // 检查音符是否与选择框相交（使用世界坐标直接比较）
                if note_end >= min_tick
                    && note.tick <= max_tick
                    && note.key >= min_key
                    && note.key <= max_key
                {
                    self.editor_state.interaction.selected_notes.insert(i);
                }
            }
        }
    }

    /// 检查是否应该开始拖动
    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_x = pos.x - start_pos.x;
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.editor_state.view.zoom_y * DRAG_START_THRESHOLD_RATIO;
        let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
        let started = distance > key_threshold;
        if started {
            tracing::info!(
                "Editor: 拖动启动 - delta=({}, {}), distance={}, threshold={}",
                delta_x,
                delta_y,
                distance,
                key_threshold
            );
        }
        started
    }

    /// 完成单音符拖动（ghost 方案）
    ///
    /// 松手时一次性将 `drag_state.delta` 应用到 `data.notes`，并发送 `LocalNoteMoved` 协作同步事件。
    /// 返回 `true` 表示音符位置确实发生了变化。
    pub(crate) fn finalize_dragging(&mut self, note_index: usize, drag_state: DragState) -> bool {
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: 单音符拖动 delta 为零，跳过提交");
            return false;
        }

        // 读取原始位置（apply 前的状态，用于协作同步事件）
        let (original_tick, original_key, length, current_track) = {
            let notes = &self.editor_state.data.notes;
            let Some(original_note) = notes.get(note_index) else {
                return false;
            };
            (
                original_note.tick,
                original_note.key,
                original_note.length,
                self.editor_state.data.current_track,
            )
        };

        let tick_offset = drag_state.delta_tick as f32;
        let key_offset = drag_state.delta_key;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // ghost 方案：流式应用 delta 到 notes 与当前 track_notes 缓存
        let modified = self
            .editor_state
            .data
            .apply_drag_state_streaming(&drag_state, max_key);
        if modified == 0 {
            tracing::debug!("Editor: 单音符拖动未产生实际变更（snap 后 delta 为零）");
            return false;
        }

        tracing::info!(
            "Editor: 音符移动完成 - original=({}, {}), offset=({}, {})",
            original_tick,
            original_key,
            tick_offset,
            key_offset
        );
        lumino_event::emit(lumino_event::Event::Window(
            lumino_event::window::Event::local_note_moved(
                original_tick,
                original_key,
                length,
                tick_offset,
                key_offset,
                current_track,
            ),
        ));
        true
    }

    // 注：原 `finalize_selection_dragging` 已移除——延迟提交方案下，松手保存到
    // `pending_drag_state`，真正提交在 `commit_pending_drag`（点击空白处或
    // `commit_current_edit` 时触发）。详见 `interaction/released.rs`。
}

use lumino_ui_constants::editor::DRAG_START_THRESHOLD_RATIO;
