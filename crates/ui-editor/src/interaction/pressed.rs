//! 鼠标按下事件处理 — 工具分发、音符编辑/绘制
//!
//! 包含：按下事件 → 工具分发 → 指针/铅笔/橡皮擦/默认工具处理
//!       音符编辑开始、绘制开始、音符音频播放、音符添加事件发射

use crate::{Editor, HitType, Note};
use lumino_core::DragState;
use lumino_core::editor_state::interaction_ops;
use lumino_core::storage::config::{EraserBehavior, SelectionBoxMode};
use lumino_event;
use lumino_message::Tool;
use lumino_ui_constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};

impl Editor {
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

        match self.editor_state.tool {
            Tool::Pointer => self.handle_pointer_pressed(pos, hit_result, snapped_tick),
            Tool::Pencil => self.handle_pencil_pressed(pos, hit_result, snapped_tick, key),
            Tool::Curve => {
                // 曲线编辑工具只能在自动化面板中使用，不能在钢琴卷帘上绘制音符
            }
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
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let selection_start_tick =
            if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct {
                tick
            } else {
                snapped_tick
            };
        if let Some((index, hit_type)) = hit_result {
            // 命中音符：如果有 pending 拖动，先提交（非累积场景）
            self.flush_pending_drag();
            if !self
                .editor_state
                .interaction
                .selected_notes
                .contains(&index)
            {
                self.editor_state.interaction.selected_notes.clear();
                self.editor_state.interaction.selected_notes.insert(index);
            }
            self.start_note_edit(index, hit_type, pos);
        } else if let Some(sel_hit) = self.hit_test_selection_box(pos) {
            // 命中选择框：根据边缘/内部分别进入调整大小或拖动状态
            match sel_hit {
                crate::SelectionHitType::Inside => {
                    // ghost 方案（累积模式）：从 selected_notes 构建 DragState
                    let note_count = self.editor_state.data.notes.len();
                    let drag_state = DragState::from_indices(
                        self.editor_state.interaction.selected_notes.iter().copied(),
                        note_count,
                        snapped_tick as i64,
                        key as i16,
                    );
                    // 累积模式：pending_drag_state 存在时，history 已在首次拖动时 push，
                    // 此处不重复 push（避免一次逻辑操作产生多条 history 记录）
                    if self.pending_drag_state.is_none() {
                        self.push_history();
                    }
                    self.editor_state.interaction.edit_state =
                        crate::EditState::DraggingSelection { drag_state };
                }
                crate::SelectionHitType::LeftEdge => {
                    // 调整大小：如果有 pending 拖动，先提交（非累积场景）
                    self.flush_pending_drag();
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionStart {
                            last_tick: snapped_tick,
                        };
                }
                crate::SelectionHitType::RightEdge => {
                    // 调整大小：如果有 pending 拖动，先提交（非累积场景）
                    self.flush_pending_drag();
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionEnd {
                            last_tick: snapped_tick,
                        };
                }
            }
        } else {
            // 点击空白处：提交 pending 拖动 + 取消框选
            self.flush_pending_drag();
            self.playback_position = snapped_tick;
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                start_tick: selection_start_tick,
                start_key: key,
                current_tick: selection_start_tick,
                current_key: key,
            };
        }
    }

    /// 提交 pending 批量拖动并清空选区（非累积场景调用）
    ///
    /// 在用户开始新操作（点击音符/调整大小/点击空白处）时调用。
    /// 累积拖动场景（框选内部命中）不调用此方法，保留 pending。
    fn flush_pending_drag(&mut self) {
        if self.pending_drag_state.is_some() {
            self.commit_pending_drag();
            self.editor_state.interaction.selected_notes.clear();
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
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);
        let selection_start_tick =
            if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct {
                tick
            } else {
                snapped_tick
            };

        match self.editor_state.view.eraser_behavior {
            EraserBehavior::Default => {
                if shift {
                    self.editor_state.interaction.selected_notes.clear();
                    self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                        start_tick: selection_start_tick,
                        start_key: key,
                        current_tick: selection_start_tick,
                        current_key: key,
                    };
                } else if hit_result.is_some() {
                    self.delete_note_at(pos);
                }
            }
            EraserBehavior::DirectSelect => {
                if shift && hit_result.is_some() {
                    self.delete_note_at(pos);
                } else {
                    self.editor_state.interaction.selected_notes.clear();
                    self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                        start_tick: selection_start_tick,
                        start_key: key,
                        current_tick: selection_start_tick,
                        current_key: key,
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
        interaction_ops::start_note_edit(
            &mut self.editor_state.data,
            &mut self.editor_state.interaction,
            index,
            hit_type,
            (pos.x, pos.y),
        );
    }

    /// 开始绘制新音符
    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        interaction_ops::start_drawing(&mut self.editor_state.interaction, snapped_tick, key);
    }

    /// 播放音符音频
    pub(crate) fn play_note_audio(&mut self, key: u16, _context: &str) {
        self.editor_state
            .interaction
            .play_note_audio(key, DEFAULT_NOTE_VELOCITY);
    }

    /// 发送新音符添加的协作同步事件
    pub(super) fn emit_note_added_event(&self, note: &Note) {
        lumino_event::emit(lumino_event::Event::Window(
            lumino_event::window::Event::local_note_added(
                note.tick,
                note.key,
                note.length,
                DEFAULT_NOTE_VELOCITY,
                DEFAULT_MIDI_CHANNEL,
                self.editor_state.data.current_track,
            ),
        ));
    }
}
