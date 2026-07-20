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
    ///
    /// **命中优先级**（关键交互逻辑）：
    /// 1. 若已有选中音符，优先检测选择框（`hit_test_selection_box`）：
    ///    - `Inside`：框选框内任意位置 → `DraggingSelection`（拖动全部选中音符）
    ///    - `LeftEdge/RightEdge`：框选框左右边缘 → `ResizingSelectionStart/End`（拉伸框选边缘）
    ///    - `None`：点击在框选框外 → 回退到音符命中检测
    /// 2. 若未命中选择框（或无选中音符），检测音符命中（`hit_test_note`）：
    ///    - 命中音符 → 单音符编辑（`ResizingStart/End/PendingDrag`）
    /// 3. 都未命中 → 点击空白处，提交 pending 拖动 + 开始新框选
    ///
    /// **修复历史**：原实现 `hit_test_note` 优先于 `hit_test_selection_box`，导致框选框内点击
    /// 若命中某个选中音符的边缘，会误进入单音符 `ResizingStart/End` 状态，框选拖动无法触发。
    /// 调整优先级后，框选框内任意位置都走框选逻辑，符合用户"按住框选框内任意位置移动即拖动"的预期。
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

        // 优先级 1：有选中音符时，先检测选择框命中
        // 选择框命中时，无论是否同时命中音符，都走框选逻辑（避免边缘误判走单音符拉伸）
        let sel_hit = if !self.editor_state.interaction.selected_notes.is_empty() {
            self.hit_test_selection_box(pos)
        } else {
            None
        };

        if let Some(sel_hit_type) = sel_hit {
            // 命中选择框：根据边缘/内部分别进入调整大小或拖动状态
            match sel_hit_type {
                crate::SelectionHitType::Inside => {
                    // ghost 方案（累积模式）：从 selected_notes 构建 DragState
                    let note_count = self.editor_state.data.notes.len();
                    let drag_state = DragState::from_indices(
                        self.editor_state.interaction.selected_notes.iter().copied(),
                        note_count,
                        snapped_tick as i64,
                        key as i16,
                    );
                    // NoteMove 操作日志化：批量拖动期间不 push 快照，
                    // 松手时构造 MoveOp 异步提交。
                    self.editor_state.interaction.edit_state =
                        crate::EditState::DraggingSelection { drag_state };
                }
                crate::SelectionHitType::LeftEdge => {
                    // 框选左边缘拉伸：先提交 pending 拖动（保留选区，要在当前选区上拉伸）
                    // 注意：不能用 flush_pending_drag（会清空 selected_notes，导致拉伸无目标）
                    if self.pending_drag_state.is_some() {
                        self.commit_pending_drag();
                    }
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionStart {
                            last_tick: snapped_tick,
                        };
                }
                crate::SelectionHitType::RightEdge => {
                    // 框选右边缘拉伸：同 LeftEdge，提交 pending 但保留选区
                    if self.pending_drag_state.is_some() {
                        self.commit_pending_drag();
                    }
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionEnd {
                            last_tick: snapped_tick,
                        };
                }
            }
        } else if let Some((index, hit_type)) = hit_result {
            // 优先级 2：未命中选择框但命中音符 → 单音符编辑
            // （点击在框选框外，或无选中音符时点击音符）
            self.flush_pending_drag();
            if !self
                .editor_state
                .interaction
                .selected_notes
                .contains(&index)
            {
                self.selection_clear();
                self.selection_insert(index);
            }
            self.start_note_edit(index, hit_type, pos);
        } else {
            // 优先级 3：都未命中 → 点击空白处，提交 pending 拖动 + 开始新框选
            self.flush_pending_drag();
            self.playback_position = snapped_tick;
            self.selection_clear();
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
            self.selection_clear();
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
                    self.selection_clear();
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
                    self.selection_clear();
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
