//! 拖动和调整大小操作

use super::{EditState, Editor};

impl Editor {
    /// 检查是否应从 PendingDrag 转换到 Dragging 状态
    pub(crate) fn try_transition_to_dragging(&mut self, pos: iced_core::Point) {
        let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.edit_state
        else {
            return;
        };

        if !self.should_start_dragging(pos, start_pos) {
            return;
        }

        let tick = self.x_to_tick(start_pos.x);
        let key = self.y_to_key(start_pos.y);
        self.push_history();
        self.edit_state = EditState::Dragging {
            note_index,
            offset_tick: tick - original_tick,
            offset_key: key.saturating_sub(original_key) as i32,
            last_played_key: original_key,
            original_tick,
            original_key,
        };
    }

    /// 根据当前编辑状态计算音符的变化量
    ///
    /// 返回 (new_tick, new_key, new_length, note_to_play)
    pub(crate) fn compute_state_changes(
        &mut self,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>, Option<u16>) {
        let snap_precision = self.state.snap_precision;
        let visible_key_count = self.state.visible_key_count;
        let mut new_tick = None;
        let mut new_key = None;
        let mut new_length = None;
        let mut note_to_play = None;

        match &mut self.edit_state {
            EditState::Selecting { current_pos: _, .. } => {
                self.update_selection();
                return (None, None, None, None);
            }
            EditState::Drawing { current_tick, .. } => {
                *current_tick = snapped_tick;
            }
            EditState::Dragging {
                offset_tick,
                offset_key,
                last_played_key,
                ..
            } => {
                let calculated_tick =
                    ((tick - *offset_tick) / snap_precision).round() * snap_precision;
                let calculated_key = (key as i32 - *offset_key)
                    .clamp(0, visible_key_count.saturating_sub(1) as i32)
                    as u16;
                new_key = Some(calculated_key);
                new_tick = Some(calculated_tick.max(0.0));

                if calculated_key != *last_played_key {
                    note_to_play = Some(calculated_key);
                    *last_played_key = calculated_key;
                }
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
                if let Some(note) = self.notes.get(*note_index) {
                    new_length = Some((snapped_tick - note.tick).max(snap_precision));
                }
            }
            _ => {}
        }

        (new_tick, new_key, new_length, note_to_play)
    }

    /// 更新框选区域中的音符选中状态
    pub(crate) fn update_selection(&mut self) {
        if let EditState::Selecting {
            start_pos,
            current_pos,
        } = self.edit_state
        {
            let min_x = start_pos.x.min(current_pos.x);
            let max_x = start_pos.x.max(current_pos.x);
            let min_y = start_pos.y.min(current_pos.y);
            let max_y = start_pos.y.max(current_pos.y);

            self.selected_notes.clear();
            for (i, note) in self.notes.iter().enumerate() {
                let note_x = self.tick_to_x(note.tick);
                let note_y = self.key_to_y(note.key);
                let note_right = self.tick_to_x(note.tick + note.length);
                let note_bottom = note_y + self.state.zoom_y;

                // 检查音符是否与选择框相交
                if note_right >= min_x && note_x <= max_x && note_bottom >= min_y && note_y <= max_y
                {
                    self.selected_notes.insert(i);
                }
            }
        }
    }

    /// 检查是否应该开始拖动
    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_x = pos.x - start_pos.x;
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.state.zoom_y * DRAG_START_THRESHOLD_RATIO;
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

    /// 完成拖动操作，发送协作同步事件
    pub(crate) fn finalize_dragging(
        &mut self,
        note_index: usize,
        original_tick: f32,
        original_key: u16,
    ) {
        let Some(note) = self.notes.get(note_index) else {
            return;
        };

        let tick_offset = note.tick - original_tick;
        let key_offset = (note.key as i16) - (original_key as i16);

        tracing::info!(
            "Editor: 音符移动完成 - original=({}, {}), current=({}, {}), offset=({}, {})",
            original_tick,
            original_key,
            note.tick,
            note.key,
            tick_offset,
            key_offset
        );

        if tick_offset.abs() > 0.001 || key_offset != 0 {
            tracing::info!("Editor: 发送 LocalNoteMoved 同步事件");
            lumino_core::event::emit(lumino_core::event::Event::Window(
                lumino_core::event::window::Event::LocalNoteMoved {
                    tick: original_tick,
                    key: original_key,
                    length: note.length,
                    tick_offset,
                    key_offset,
                    track_index: self.current_track,
                },
            ));
        } else {
            tracing::info!("Editor: 音符偏移量为零，跳过同步");
        }
    }
}

use crate::constants::editor::DRAG_START_THRESHOLD_RATIO;
