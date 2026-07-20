//! 鼠标移动事件处理 — 实时更新编辑状态
//!
//! 包含：鼠标移动时的状态更新、编辑变化计算、变化值应用

use crate::{EditState, Editor};
use lumino_core::editor_state::interaction_ops;

impl Editor {
    /// 处理鼠标移动事件
    pub(crate) fn handle_moved(&mut self, pos: iced_core::Point) {
        crate::puffin_profiler::moved_handle();
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        // 框选/拖拽过程中 hover 判定无意义，且会触发空间索引重建/线性扫描或
        // collect_ghost_indices 的 O(N) 遍历（1600W 选中音符），跳过以提升性能。
        // Dragging/DraggingSelection 状态下 mouse_interaction 直接返回 Grabbing，
        // 不依赖 hover 状态，因此跳过是安全的。
        let hover = if matches!(
            self.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
                | EditState::Dragging { .. }
                | EditState::DraggingSelection { .. }
        ) {
            None
        } else {
            self.hit_test_note(pos)
        };
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
            *current_tick = if self.editor_state.view.selection_box_mode
                == lumino_core::storage::config::SelectionBoxMode::Direct
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
        crate::puffin_profiler::calculate_edit_changes();
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
        crate::puffin_profiler::apply_note_changes();
        if interaction_ops::apply_note_changes(
            &mut self.editor_state.data,
            &self.editor_state.interaction.edit_state,
            new_tick,
            new_key,
            new_length,
        ) {
            // ghost 方案：单音符 Resizing 期间不每帧重建空间索引。
            // apply_note_changes 仅在 ResizingStart/End 返回 true（其他状态 noop），
            // notes 已被改，渲染基于新 notes 正确。空间索引在松手时
            // （released.rs ResizingStart/End 分支）一次性 mark_notes_changed 重建。
            // **性能关键**：1000W 音符建树 124ms，每帧重建 = 灾难。
            crate::puffin_profiler::mark_ghost_dirty();
            self.mark_ghost_dirty();
        }
    }
}
