use super::{EditState, Editor, HitType, Note};
use crate::constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};
use crate::message::{AudioAction, EditorAction};
use crate::toolbar::Tool;
use lumino_core::event;
use lumino_core::storage::config::EraserBehavior;

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
            EditorAction::Cut => self.cut_selected_notes(),
            EditorAction::Copy => {
                self.copy_selected_notes();
            }
            EditorAction::Paste => self.paste_notes_from_clipboard(),
            EditorAction::SelectAll => self.select_all_notes(),
            EditorAction::Undo => {
                self.undo();
            }
            EditorAction::Redo => {
                self.redo();
            }
            EditorAction::Scrubbed { tick } => {
                self.playback_position = tick;
            }
        }
    }

    /// 处理鼠标按下事件
    pub(crate) fn handle_pressed(&mut self, pos: iced_core::Point, shift: bool) {
        if !self.is_inside_canvas(pos) {
            return;
        }

        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        self.handle_tool_pressed(pos, shift, snapped_tick, key);
    }

    /// 根据当前工具处理鼠标按下事件
    pub(crate) fn handle_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        snapped_tick: f32,
        key: u16,
    ) {
        let hit_result = self.hit_test_note(pos);

        match self.current_tool {
            Tool::Pointer => self.handle_pointer_pressed(pos, hit_result, snapped_tick),
            Tool::Pencil => self.handle_pencil_pressed(pos, hit_result, snapped_tick, key),
            Tool::Eraser => self.handle_eraser_pressed(pos, shift, hit_result),
            _ => self.handle_default_tool_pressed(pos, hit_result, snapped_tick, key),
        }
    }

    /// 指针工具：框选或编辑现有音符
    pub(crate) fn handle_pointer_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
    ) {
        if let Some((index, hit_type)) = hit_result {
            if !self.selected_notes.contains(&index) {
                self.selected_notes.clear();
                self.selected_notes.insert(index);
            }
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.playback_position = snapped_tick;
            self.selected_notes.clear();
            self.edit_state = EditState::Selecting {
                start_pos: pos,
                current_pos: pos,
            };
        }
    }

    /// 铅笔工具：放置新音符或编辑现有音符
    pub(crate) fn handle_pencil_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    /// 橡皮擦工具：删除音符
    pub(crate) fn handle_eraser_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        hit_result: Option<(usize, HitType)>,
    ) {
        match self.state.eraser_behavior {
            EraserBehavior::Default => {
                if shift {
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                } else if hit_result.is_some() {
                    self.delete_note_at(pos);
                }
            }
            EraserBehavior::DirectSelect => {
                if shift && hit_result.is_some() {
                    self.delete_note_at(pos);
                } else {
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                }
            }
        }
    }

    /// 其他工具：默认使用铅笔工具逻辑
    pub(crate) fn handle_default_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    /// 开始编辑现有音符
    fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: iced_core::Point) {
        match hit_type {
            HitType::Start => {
                self.push_history();
                let note = &self.notes[index];
                self.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.push_history();
                self.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                let note = &self.notes[index];
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
    pub(crate) fn play_note_audio(&mut self, key: u16, _context: &str) {
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity: DEFAULT_NOTE_VELOCITY,
        });
    }

    /// 处理鼠标移动事件
    pub(crate) fn handle_moved(&mut self, pos: iced_core::Point) {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        self.hover_state = self.hit_test_note(pos);

        if let EditState::Scrubbing = self.edit_state {
            self.playback_position = snapped_tick;
            return;
        }

        if let EditState::Selecting { current_pos, .. } = &mut self.edit_state {
            *current_pos = pos;
        }

        let (new_tick, new_key, new_length) =
            self.calculate_edit_changes(pos, tick, key, snapped_tick);
        self.apply_note_changes(new_tick, new_key, new_length);
    }

    /// 计算编辑状态的变化值
    pub(crate) fn calculate_edit_changes(
        &mut self,
        pos: iced_core::Point,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>) {
        self.try_transition_to_dragging(pos);

        let (new_tick, new_key, new_length, note_to_play) =
            self.compute_state_changes(tick, key, snapped_tick);

        if let Some(k) = note_to_play {
            self.play_note_audio(k, "拖动变化");
        }

        (new_tick, new_key, new_length)
    }

    /// 应用音符变化
    pub(crate) fn apply_note_changes(
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
    pub(crate) fn handle_released(&mut self) {
        match self.edit_state {
            EditState::Selecting { .. } => {
                if self.current_tool == Tool::Eraser {
                    self.delete_selected_notes();
                } else {
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
            EditState::PendingDrag { .. } => {}
            EditState::Dragging {
                note_index,
                original_tick,
                original_key,
                ..
            } => {
                self.finalize_dragging(note_index, original_tick, original_key);
            }
            EditState::ResizingStart { .. } | EditState::ResizingEnd { .. } => {
                tracing::debug!("Editor: 音符调整大小完成");
            }
            _ => {}
        }
        self.edit_state = EditState::Idle;
    }

    /// 完成绘制新音符
    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, self.state.default_note_length)
        };
        let length = length.max(self.state.snap_precision);

        self.push_history();
        let note = Note::new(tick, key, length);
        self.notes.push_back(note.clone());
        self.track_notes
            .insert(self.current_track, self.notes.clone());

        self.emit_note_added_event(&note);
        tracing::debug!(
            "编辑器: 已保存 {} 个音符到音轨 {}",
            self.notes.len(),
            self.current_track
        );
        self.mark_notes_changed();
    }

    /// 发送新音符添加的协作同步事件
    fn emit_note_added_event(&self, note: &Note) {
        event::emit(event::Event::Window(event::window::Event::LocalNoteAdded {
            tick: note.tick,
            key: note.key,
            length: note.length,
            velocity: DEFAULT_NOTE_VELOCITY,
            channel: DEFAULT_MIDI_CHANNEL,
            track_index: self.current_track,
        }));
    }

    /// 处理滚动事件
    pub(crate) fn handle_scrolled(&mut self, delta_x: f32, delta_y: f32) {
        let new_scroll_y = self.state.scroll_y - delta_y;
        self.set_scroll_y(new_scroll_y);

        if delta_x != 0.0 {
            let new_scroll_x = self.state.scroll_x - delta_x;
            self.set_scroll_x(new_scroll_x);
        }
    }

    /// 处理双击事件
    pub(crate) fn handle_double_clicked(&mut self, pos: iced_core::Point) {
        if self.is_inside_canvas(pos)
            && let Some((index, _)) = self.hit_test_note(pos)
        {
            self.delete_note_by_index(index);
        }
    }

    /// 处理删除键按下事件
    pub(crate) fn handle_delete_pressed(&mut self) {
        if let Some((index, _)) = self.hover_state {
            self.delete_note_by_index(index);
        }
    }
}
