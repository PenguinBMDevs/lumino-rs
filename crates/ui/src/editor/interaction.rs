use super::{EditState, Editor, HitType, Note};
use crate::constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};
use crate::event;
use crate::message::EditorAction;
use crate::toolbar::Tool;
use lumino_core::storage::config::{EraserBehavior, SelectionBoxMode};

impl Editor {
    /// 主入口：处理编辑器动作
    pub fn handle_action(&mut self, action: EditorAction) {
        self.editor_state.interaction.pending_audio_actions.clear();

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
                // 固定指示线模式下：更新 fixed_indicator_position 到当前屏幕位置
                // 使指示线出现在点击位置，而不是强制回到预设点
                if self.editor_state.auto_scroll.mode
                    == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                {
                    let v = &self.editor_state.view;
                    let new_pos = (tick * v.zoom_x - v.scroll_x).max(0.0) as u32;
                    self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                }
            }
            EditorAction::IndicatorDragStart { x } => {
                // 固定指示线模式下拖拽指示线：更新固定位置到鼠标位置
                let new_pos = (x - self.editor_state.view.keyboard_width).max(0.0) as u32;
                self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                // 同时更新播放位置到鼠标对应的 tick
                let tick = self.x_to_tick(x);
                let snapped_tick = self.snap_tick(tick).max(0.0);
                self.playback_position = snapped_tick;
            }
            EditorAction::IndicatorDragMove { x } => {
                // 拖拽中：持续更新固定位置和播放位置
                let new_pos = (x - self.editor_state.view.keyboard_width).max(0.0) as u32;
                self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                let tick = self.x_to_tick(x);
                let snapped_tick = self.snap_tick(tick).max(0.0);
                self.playback_position = snapped_tick;
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

        match self.editor_state.tool {
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
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let selection_start_tick =
            if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct {
                tick
            } else {
                snapped_tick
            };
        if let Some((index, hit_type)) = hit_result {
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
                super::SelectionHitType::Inside => {
                    self.push_history();
                    self.editor_state.interaction.edit_state = EditState::DraggingSelection {
                        last_tick: snapped_tick,
                        last_key: key,
                    };
                }
                super::SelectionHitType::LeftEdge => {
                    self.push_history();
                    self.editor_state.interaction.edit_state = EditState::ResizingSelectionStart {
                        last_tick: snapped_tick,
                    };
                }
                super::SelectionHitType::RightEdge => {
                    self.push_history();
                    self.editor_state.interaction.edit_state = EditState::ResizingSelectionEnd {
                        last_tick: snapped_tick,
                    };
                }
            }
        } else {
            self.playback_position = snapped_tick;
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.edit_state = EditState::Selecting {
                start_tick: selection_start_tick,
                start_key: key,
                current_tick: selection_start_tick,
                current_key: key,
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
                    self.editor_state.interaction.edit_state = EditState::Selecting {
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
                    self.editor_state.interaction.edit_state = EditState::Selecting {
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
        self.editor_state
            .start_note_edit(index, hit_type, (pos.x, pos.y));
    }

    /// 开始绘制新音符
    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.editor_state.start_drawing(snapped_tick, key);
    }

    /// 播放音符音频
    pub(crate) fn play_note_audio(&mut self, key: u16, _context: &str) {
        self.editor_state
            .interaction
            .play_note_audio(key, DEFAULT_NOTE_VELOCITY);
    }

    /// 处理鼠标移动事件
    pub(crate) fn handle_moved(&mut self, pos: iced_core::Point) {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        // 先计算 hit_test（不借用 editor_state），再赋值
        let hover = self.hit_test_note(pos);
        self.editor_state.interaction.hover_state = hover;

        if let EditState::Scrubbing = self.editor_state.interaction.edit_state {
            self.playback_position = snapped_tick;
            return;
        }

        if let EditState::Selecting {
            current_tick,
            current_key,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            // 直接跟随模式：框选框使用原始坐标，不吸附到网格
            *current_tick = if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct
            {
                tick
            } else {
                snapped_tick
            };
            *current_key = key;
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
        if self
            .editor_state
            .apply_note_changes(new_tick, new_key, new_length)
        {
            self.spatial.note_index_dirty.set(true);
        }
    }

    /// 处理鼠标释放事件
    pub(crate) fn handle_released(&mut self) {
        let edit_state = std::mem::take(&mut self.editor_state.interaction.edit_state);
        match edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
            } => {
                if self.editor_state.tool == Tool::Eraser {
                    self.delete_notes_in_selection_box(
                        start_tick,
                        start_key,
                        current_tick,
                        current_key,
                    );
                } else {
                    tracing::debug!(
                        "框选结束，选中 {} 个音符",
                        self.editor_state.interaction.selected_notes.len()
                    );
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
            EditState::DraggingSelection { .. }
            | EditState::ResizingSelectionStart { .. }
            | EditState::ResizingSelectionEnd { .. } => {
                tracing::debug!("Editor: 选择框批量编辑完成");
            }
            _ => {}
        }
    }

    /// 完成绘制新音符
    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let v = &self.editor_state.view;
        if let Some(note) = self.editor_state.data.finish_drawing(
            start_tick,
            key,
            current_tick,
            v.snap_precision,
            v.default_note_length,
        ) {
            self.emit_note_added_event(&note);
            self.mark_notes_changed();
        }
    }

    /// 发送新音符添加的协作同步事件
    fn emit_note_added_event(&self, note: &Note) {
        event::emit(event::Event::Window(
            event::window::Event::local_note_added(
                note.tick,
                note.key,
                note.length,
                DEFAULT_NOTE_VELOCITY,
                DEFAULT_MIDI_CHANNEL,
                self.editor_state.data.current_track,
            ),
        ));
    }

    /// 处理滚动事件（鼠标滚轮）
    /// 使用平滑滚动动画，不直接设置位置。
    ///
    /// 垂直方向反转：向上滚动（delta_y > 0）应减小 scroll_y（显示更高音区）。
    /// 水平方向保持标准方向：正 delta_x 向右滚动（scroll_x 增大，显示更后音符）。
    /// 两个方向均钳制到有效范围，防止平滑滚动越界。
    pub(crate) fn handle_scrolled(&mut self, delta_x: f32, delta_y: f32) {
        let state = &mut self.editor_state;
        let v = &mut state.view;
        let max_x =
            (state.max_scroll.0 - (state.canvas.size_x - v.keyboard_width).max(0.0)).max(0.0);
        let max_y = (state.max_scroll.1 - (state.canvas.size_y - v.ruler_height).max(0.0)).max(0.0);
        let target_x = (v.scroll_x + delta_x).clamp(0.0, max_x);
        let target_y = (v.scroll_y - delta_y).clamp(0.0, max_y);
        v.smooth_scroll.set_target(target_x, target_y);
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
        if self.editor_state.handle_delete_pressed().is_some() {
            self.mark_notes_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::*;

    #[test]
    fn test_editor_action_dispatch() {
        let mut editor = Editor::new();
        assert!(!editor.notes_changed());

        // DeletePressed 不应 panic（空 editor 下无 hover note）
        editor.handle_action(super::EditorAction::DeletePressed);
        assert!(!editor.notes_changed()); // 没有选中音符，notes_changed 不应变化

        // Moved 不应 panic
        editor.handle_action(super::EditorAction::Moved(iced_core::Point::new(
            100.0, 200.0,
        )));
    }

    #[test]
    fn test_memory_breakdown_empty() {
        let editor = Editor::new();
        let mem = editor.memory_breakdown();
        assert_eq!(mem.notes_bytes, 0);
        assert_eq!(mem.track_notes_count, 0);
    }

    #[test]
    fn test_update_cursor_position() {
        let mut editor = Editor::new();
        editor.update_cursor_position(Some(iced_core::Point::new(100.0, 200.0)));
        // 不应 panic
        editor.update_cursor_position(None);
    }

    #[test]
    fn test_spatial_index_default() {
        let state = crate::editor::SpatialIndexState::default();
        assert!(state.note_index.borrow().is_none());
        assert!(!state.note_index_dirty.get()); // 默认未脏
        assert!(state.query_cache.borrow().is_empty());
    }

    #[test]
    fn test_cache_invalidation() {
        use crate::editor::CacheInvalidation;
        assert_eq!(
            CacheInvalidation::GRID.0 & CacheInvalidation::ALL.0,
            CacheInvalidation::GRID.0
        );
        assert_eq!(
            CacheInvalidation::NONE.0 | CacheInvalidation::KEYBOARD.0,
            CacheInvalidation::KEYBOARD.0
        );
    }

    // ── 平滑滚动方向与边界 ──

    /// 默认 view: zoom_x=0.1, zoom_y=20, total_ticks=768000,
    /// visible_key_count=128 → max_scroll=(76800, 2560)
    const DEFAULT_MAX_X: f32 = 76800.0;
    const DEFAULT_MAX_Y: f32 = 2560.0;

    #[test]
    fn test_scroll_vertical_direction_up() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        // 先滚到中间位置，确保减量方向可观察
        editor.editor_state.view.scroll_y = 500.0;
        editor.editor_state.view.smooth_scroll.target_y = 500.0;

        // 向上滚 → delta_y > 0 → scroll_y 应减小（显示更高音区）
        editor.handle_scrolled(0.0, 50.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_y < 500.0,
            "向上滚动应减小 scroll_y，但 target_y={} >= 500",
            editor.editor_state.view.smooth_scroll.target_y
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
        // target 不应小于 0（被下界钳制）
        assert!(
            editor.editor_state.view.smooth_scroll.target_y >= 0.0,
            "target_y 不应为负，实际={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_vertical_direction_down() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向下滚 → delta_y < 0 → scroll_y 应增大（显示更低音区）
        editor.handle_scrolled(0.0, -50.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_y > 0.0,
            "向下滚动应增大 scroll_y，但 target_y={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
    }

    #[test]
    fn test_scroll_horizontal_direction_right() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向右滚 → delta_x > 0 → scroll_x 应增大（显示更后音符）
        editor.handle_scrolled(50.0, 0.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_x > 0.0,
            "向右滚动应增大 scroll_x，但 target_x={}",
            editor.editor_state.view.smooth_scroll.target_x
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
    }

    #[test]
    fn test_scroll_horizontal_direction_left() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        // 先设到中间位置，确保减量方向可观察
        editor.editor_state.view.scroll_x = 500.0;
        editor.editor_state.view.smooth_scroll.target_x = 500.0;

        editor.handle_scrolled(-100.0, 0.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_x < 500.0,
            "向左滚动应减小 scroll_x，但 target_x={} >= 500",
            editor.editor_state.view.smooth_scroll.target_x
        );
    }

    #[test]
    fn test_scroll_boundary_vertical_upper() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向下滚很大 → target_y 应被上界钳制到 max_y
        // max_y = 2560 - (500 - 24).max(0) = 2560 - 476 = 2084
        editor.handle_scrolled(0.0, -999999.0);
        let max_y = (DEFAULT_MAX_Y
            - (editor.editor_state.canvas.size_y - editor.editor_state.view.ruler_height).max(0.0))
        .max(0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, max_y,
            "向下滚到极限应停在 max_y={}，实际 target_y={}",
            max_y, editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_boundary_horizontal_upper() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向右滚很大 → target_x 应被上界钳制到 max_x
        // max_x = 76800 - (1000 - 120).max(0) = 76800 - 880 = 75920
        editor.handle_scrolled(999999.0, 0.0);
        let max_x = (DEFAULT_MAX_X
            - (editor.editor_state.canvas.size_x - editor.editor_state.view.keyboard_width)
                .max(0.0))
        .max(0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, max_x,
            "向右滚到极限应停在 max_x={}，实际 target_x={}",
            max_x, editor.editor_state.view.smooth_scroll.target_x
        );
    }

    #[test]
    fn test_scroll_boundary_lower() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 从 scroll=0 向上滚 → target 不应低于 0
        editor.handle_scrolled(-999999.0, 0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, 0.0,
            "向左滚到极限应停在 0，实际 target_x={}",
            editor.editor_state.view.smooth_scroll.target_x
        );

        editor.handle_scrolled(0.0, 999999.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, 0.0,
            "向上滚到极限应停在 0，实际 target_y={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_noop_on_zero_delta() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        let initial_x = editor.editor_state.view.smooth_scroll.target_x;
        let initial_y = editor.editor_state.view.smooth_scroll.target_y;

        editor.handle_scrolled(0.0, 0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, initial_x,
            "delta=0 不应改变 target_x"
        );
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, initial_y,
            "delta=0 不应改变 target_y"
        );
    }
}
