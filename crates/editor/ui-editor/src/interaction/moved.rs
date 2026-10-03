//! 鼠标移动事件处理 — 实时更新编辑状态
//!
//! 包含：鼠标移动时的状态更新、编辑变化计算、变化值应用

use crate::{EditState, Editor};
use lumino_editor_state::LineToolInteraction;
use lumino_editor_state::ShapeToolInteraction;
use lumino_editor_state::editor_state::interaction_ops;

impl Editor {
    /// 处理鼠标移动事件
    pub(crate) fn handle_moved(&mut self, pos: iced_core::Point) {
        crate::puffin_profiler::moved_handle();
        // 画刷矢量笔画：落笔绘制/整体拖动中，跟随鼠标更新笔画几何（不写 document）
        if self.editor_state.brush_tool.is_active() {
            self.handle_brush_moved(pos);
            return;
        }
        // 图形选中工具：拖动已绘制图形（仅更新叠加层预览偏移，不改 document）
        if self.editor_state.shape_select.is_dragging() {
            let raw_tick = self.pos_to_tick(pos);
            let raw_key = self.pos_to_raw_key(pos);
            self.handle_shape_select_moved(raw_tick, raw_key);
            return;
        }
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
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

        // 曲线工具贝塞尔路径模式：锚点/控制柄拖动中（纵向转置）
        if self.editor_state.tool == lumino_message::Tool::Curve
            && self.editor_state.line_tool.interaction != LineToolInteraction::None
        {
            let raw_tick = self.pos_to_tick(pos);
            let raw_key = self.pos_to_raw_key(pos);
            // 横向原逻辑：snapped_tick 来自 tick 吸附，key 取整，raw 为自由值
            // 纵向保持同语义，仅轴互换
            self.handle_line_tool_moved(snapped_tick, key as f32, raw_tick, raw_key);
            return;
        }

        // 文字工具：已放置框的拖拽移动（中间实心区），优先于 Selecting 分支
        if self.editor_state.tool == lumino_message::Tool::Text
            && self.editor_state.text_tool.active
            && self.editor_state.text_tool.is_dragging()
        {
            self.handle_text_tool_box_move(pos);
            return;
        }

        // 形状工具：拖拽拉框中（横向 / 纵向卷帘统一走逻辑坐标）
        if self.editor_state.tool == lumino_message::Tool::Shape
            && self.editor_state.shape_tool.interaction != ShapeToolInteraction::None
        {
            // Shift 按住：使用原始浮点坐标（绕过 key/音符精度吸附，自由跟随鼠标）；
            // 否则：网格吸附坐标。正图形约束在几何层基于该坐标计算。
            let raw_key = self.pos_to_raw_key(pos);
            let (dt, dk) = if self.shift_pressed() {
                (tick, raw_key)
            } else {
                (snapped_tick, key as f32)
            };
            self.handle_shape_tool_moved(dt, dk);
            return;
        }

        // 文字工具：拖拽中实时更新文本框（Selecting 态）
        if self.editor_state.tool == lumino_message::Tool::Text
            && matches!(
                self.editor_state.interaction.edit_state,
                EditState::Selecting { .. }
            )
        {
            self.handle_text_tool_moved(pos);
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

        // 预计算垂直/水平所需的 Y 值，避免在 &mut borrow 期间再借 self
        let is_y_select = self.editor_state.tool == lumino_message::Tool::PointerYSelect;
        let is_vertical = self.editor_state.is_vertical_roll;
        let tick_y_vertical = if is_vertical {
            self.tick_to_y_vertical(tick)
        } else {
            0.0
        };
        let key_y_horizontal = if !is_vertical {
            let view = &self.editor_state.view;
            view.key_to_y(key) + view.zoom_y
        } else {
            0.0
        };
        // 框选 X 向量化：锚点固定，两端由 (锚点, 鼠标 tick) 统一推算 —— **单元覆盖式**。
        //
        // 与历史 bug 的区别（评审勿改回单侧吸附）：
        // - 曾用 `snap_tick_forward`（1/4 提前吸附）→ 移动端单向外扩，反向拖动时
        //   左边界最多外扩整整一个精度单元，选中鼠标从未扫过的音符；
        // - 曾用单侧 `snap_tick`(floor) 同时刷两端 → 大端内缩（框内漏选），
        //   小端在反向拖动时外扩一整格（同样误选）。
        // 本方案两端同口径覆盖「鼠标所在单元」，配合命中半开区间 `[min, max)`：
        // 框边界落在格线、贴边音符不误选、鼠标扫过的单元内零漏选。
        //
        // 锚点为 `None`（非按下路径构造的 Selecting，例如测试直接赋值）→ 保持
        // 原始精确语义，避免影响非用户路径。
        let marquee_anchor = self.marquee_anchor_tick;
        let marquee_edges = marquee_anchor.map(|anchor| self.marquee_edges(anchor, tick));

        if let EditState::Selecting {
            start_tick,
            current_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            if let Some((sel_start, sel_current)) = marquee_edges {
                *start_tick = sel_start;
                *current_tick = sel_current;
            } else {
                *current_tick = tick;
            }
            if !is_y_select {
                *current_key = key;
                if is_vertical {
                    // 纵向：Y 为时间轴，current_y 为 tick 的屏幕 Y（零高初始，后续随 tick 扩展）
                    *current_y = tick_y_vertical;
                } else {
                    // 上下精度 = 单个 key：current_y 对齐到 key 线
                    //（key_to_y(key) + zoom_y 为该 key 的底边，覆盖完整整数 key 范围）
                    *current_y = key_y_horizontal;
                }
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
