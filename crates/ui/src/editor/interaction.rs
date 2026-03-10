use crate::constants::editor::DRAG_START_THRESHOLD_RATIO;

use super::{EditState, Editor, HitType};
use crate::message::{AudioAction, EditorAction};
use crate::toolbar::Tool;

impl Editor {
    /// 主入口：处理编辑器动作
    pub fn handle_action(&mut self, action: EditorAction) {
        self.pending_audio_actions.clear();

        match action {
            EditorAction::Pressed { pos, shift } => self.handle_pressed(pos, shift),
            EditorAction::Moved(pos) => self.handle_moved(pos),
            EditorAction::Released => self.handle_released(),
            EditorAction::Scrolled { delta_x, delta_y } => self.handle_scrolled(delta_x, delta_y),
            EditorAction::DoubleClicked(pos) => self.handle_double_clicked(pos),
            EditorAction::DeletePressed => self.handle_delete_pressed(),
        }
    }

    /// 处理鼠标按下事件
    fn handle_pressed(&mut self, pos: iced_core::Point, shift: bool) {
        if !self.is_inside_canvas(pos) {
            return;
        }

        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        match self.current_tool {
            Tool::Pointer => {
                // 指针工具：框选或编辑现有音符
                if let Some((index, hit_type)) = self.hit_test_note(pos) {
                    // 点击到音符：进入编辑模式（支持多选拖动）
                    if !self.selected_notes.contains(&index) {
                        // 如果没有按住 Ctrl，清除之前的选中
                        self.selected_notes.clear();
                        self.selected_notes.insert(index);
                    }
                    self.start_note_edit(index, hit_type, pos);
                } else {
                    // 点击空白处：开始框选
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                }
            }
            Tool::Pencil => {
                // 铅笔工具：放置新音符或编辑现有音符
                if let Some((index, hit_type)) = self.hit_test_note(pos) {
                    self.start_note_edit(index, hit_type, pos);
                } else {
                    self.start_drawing(snapped_tick, key);
                }
            }
            Tool::Eraser => {
                // 橡皮擦工具：删除音符
                if shift {
                    // Shift+点击：进入框选删除模式
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                } else {
                    // 普通点击：删除单个音符
                    self.delete_note_at(pos);
                }
            }
            _ => {
                // 其他工具：暂时使用铅笔工具逻辑
                if let Some((index, hit_type)) = self.hit_test_note(pos) {
                    self.start_note_edit(index, hit_type, pos);
                } else {
                    self.start_drawing(snapped_tick, key);
                }
            }
        }
    }

    /// 开始编辑现有音符
    fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: iced_core::Point) {
        let note = &self.notes[index];

        match hit_type {
            HitType::Start => {
                self.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                self.edit_state = EditState::PendingDrag {
                    note_index: index,
                    start_pos: pos,
                    original_tick: note.tick,
                    original_key: note.key,
                };
                self.play_note_audio(note.key, "点击音符");
            }
        }
    }

    /// 开始绘制新音符
    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
        self.play_note_audio(key, "新音符");
    }

    /// 播放音符音频
    fn play_note_audio(&mut self, key: u16, context: &str) {
        tracing::debug!("Editor: 发送 PlayNote ({}) key={}", context, key);
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity: 100,
        });
    }

    /// 处理鼠标移动事件
    fn handle_moved(&mut self, pos: iced_core::Point) {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        self.hover_state = self.hit_test_note(pos);

        let (new_tick, new_key, new_length) =
            self.calculate_edit_changes(pos, tick, key, snapped_tick);
        self.apply_note_changes(new_tick, new_key, new_length);
    }

    /// 计算编辑状态的变化值
    fn calculate_edit_changes(
        &mut self,
        pos: iced_core::Point,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>) {
        let mut new_tick = None;
        let mut new_key = None;
        let mut new_length = None;
        let mut note_to_play = None;

        let snap_precision = self.state.snap_precision;
        let visible_key_count = self.state.visible_key_count;

        // 先处理可能改变 edit_state 的情况
        if let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.edit_state
            && self.should_start_dragging(pos, start_pos)
        {
            let tick = self.x_to_tick(start_pos.x);
            let key = self.y_to_key(start_pos.y);
            self.edit_state = EditState::Dragging {
                note_index,
                offset_tick: tick - original_tick,
                offset_key: key as i32 - original_key as i32,
                last_played_key: original_key,
            };
        }

        match &mut self.edit_state {
            EditState::Selecting { current_pos, .. } => {
                *current_pos = pos;
                // 更新选中的音符
                self.update_selection();
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
                let calculated_key =
                    (key as i32 - *offset_key).clamp(0, visible_key_count as i32 - 1) as u16;
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

        // 在 match 之后播放音频，避免借用冲突
        if let Some(k) = note_to_play {
            self.play_note_audio(k, "拖动变化");
        }

        (new_tick, new_key, new_length)
    }

    /// 更新框选区域中的音符选中状态
    fn update_selection(&mut self) {
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
                if note_right >= min_x
                    && note_x <= max_x
                    && note_bottom >= min_y
                    && note_y <= max_y
                {
                    self.selected_notes.insert(i);
                }
            }
        }
    }

    /// 检查是否应该开始拖动
    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.state.zoom_y * DRAG_START_THRESHOLD_RATIO;
        delta_y.abs() > key_threshold
    }

    /// 应用音符变化
    fn apply_note_changes(
        &mut self,
        new_tick: Option<f32>,
        new_key: Option<u16>,
        new_length: Option<f32>,
    ) {
        let note_index = match self.edit_state {
            EditState::Dragging { note_index, .. }
            | EditState::ResizingStart { note_index, .. }
            | EditState::ResizingEnd { note_index, .. } => note_index,
            _ => return,
        };

        if let Some(note) = self.notes.get_mut(note_index) {
            if let Some(t) = new_tick {
                note.tick = t;
            }
            if let Some(k) = new_key {
                note.key = k;
            }
            if let Some(l) = new_length {
                note.length = l;
            }
        }
    }

    /// 处理鼠标释放事件
    fn handle_released(&mut self) {
        match self.edit_state {
            EditState::Selecting { .. } => {
                // 框选结束
                if self.current_tool == Tool::Eraser {
                    // 橡皮擦工具：删除选中的音符
                    self.delete_selected_notes();
                } else {
                    // 指针工具：保持选中状态
                    tracing::debug!("框选结束，选中 {} 个音符", self.selected_notes.len());
                }
            }
            EditState::Drawing {
                start_tick,
                key,
                current_tick,
            } => {
                self.finish_drawing(start_tick, key, current_tick);
            }
            EditState::PendingDrag { .. } => {
                // 只是点击，没有拖动，保持音符不变
            }
            _ => {}
        }
        self.edit_state = EditState::Idle;
    }

    /// 完成绘制新音符
    fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, self.state.default_note_length)
        };

        let length = length.max(self.state.snap_precision);
        self.notes.push(super::Note::new(tick, key, length));
        self.track_notes
            .insert(self.current_track, self.notes.clone());

        tracing::debug!(
            "Editor: saved {} notes to track {}",
            self.notes.len(),
            self.current_track
        );
    }

    /// 处理滚动事件
    fn handle_scrolled(&mut self, delta_x: f32, delta_y: f32) {
        let new_scroll_y = self.state.scroll_y - delta_y;
        self.set_scroll_y(new_scroll_y);

        if delta_x != 0.0 {
            let new_scroll_x = self.state.scroll_x - delta_x;
            self.set_scroll_x(new_scroll_x);
        }
    }

    /// 处理双击事件
    fn handle_double_clicked(&mut self, pos: iced_core::Point) {
        if self.is_inside_canvas(pos)
            && let Some((index, _)) = self.hit_test_note(pos)
        {
            self.delete_note_by_index(index);
        }
    }

    /// 处理删除键按下事件
    fn handle_delete_pressed(&mut self) {
        if let Some((index, _)) = self.hover_state {
            self.delete_note_by_index(index);
        }
    }
}
