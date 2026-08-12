//! 鼠标移动事件处理 — 实时更新编辑状态
//!
//! 包含：鼠标移动时的状态更新、编辑变化计算、变化值应用

use crate::{EditState, Editor};
use lumino_editor_state::LineToolInteraction;
use lumino_editor_state::editor_state::interaction_ops;

impl Editor {
    /// 处理鼠标移动事件
    pub(crate) fn handle_moved(&mut self, pos: iced_core::Point) {
        crate::puffin_profiler::moved_handle();
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        // 图片转 MIDI 放置模式：移动/拉伸（框选 Selecting 复用下方 EditState::Selecting 逻辑）
        let i2m_interaction = self.editor_state.image_to_midi.interaction;
        if self.editor_state.image_to_midi.is_active()
            && i2m_interaction != lumino_editor_state::I2mInteraction::Selecting
            && i2m_interaction != lumino_editor_state::I2mInteraction::None
        {
            self.handle_i2m_moved(snapped_tick, key as f32);
            return;
        }

        // 曲线工具贝塞尔路径模式：锚点/控制柄拖动中
        if self.editor_state.tool == lumino_message::Tool::Curve
            && self.editor_state.line_tool.interaction != LineToolInteraction::None
        {
            self.handle_line_tool_moved(
                snapped_tick,
                key as f32,
                self.x_to_tick(pos.x),
                self.raw_y_to_key(pos.y),
            );
            return;
        }

        // 框选/拖拽过程中 hover 判定无意义，且会触发空间索引重建/线性扫描或
        // collect_ghost_indices 的 O(N) 遍历（1600W 选中音符），跳过以提升性能。
        // Dragging/DraggingSelection 状态下 mouse_interaction 直接返回 Grabbing，
        // 不依赖 hover 状态，因此跳过是安全的。
        //
        // 松手后（pending_drag_state 非空，edit_state=Idle），hit_test_note 内部
        // collect_ghost_indices 仍会遍历 pending.selected_indices() 全量选中音符
        // 构建 HashSet + 排序，1600W 场景下 ~6.7s/帧。此时 hover 无实际意义
        //（用户未在交互），直接跳过。
        let hover = if self.pending_drag_state.is_some()
            || self.pending_copy_drag_state.is_some()
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::Selecting { .. }
                    | EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::DraggingSelectionCopy { .. }
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
            start_tick,
            current_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            // Y 向框选工具：X 维度按用户精度 snap，Y 维度保持全范围不动
            let is_y_select = self.editor_state.tool == lumino_message::Tool::PointerYSelect;
            let view = &self.editor_state.view;
            // 左右精度 = 用户设置的音符放置精度（Direct/Spring 模式统一）；
            // 正向拖动使用"1/4 提前"吸附（鼠标进入精度单元的前 1/4 处即扩展，
            // 避免粗精度下框选框扩展滞后于鼠标），反向拖动保持 floor 吸附
            *current_tick = if tick >= *start_tick {
                view.snap_tick_forward(tick)
            } else {
                snapped_tick
            };
            if !is_y_select {
                // 上下精度 = 单个 key：current_y 对齐到 key 线
                //（key_to_y(key) + zoom_y 为该 key 的底边，覆盖完整整数 key 范围）
                *current_key = key;
                *current_y = view.key_to_y(key) + view.zoom_y;
            }
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
