//! 鼠标释放事件处理 — 完成编辑操作
//!
//! 包含：释放事件的匹配分发、绘制完成的收尾工作

use crate::drag::compute_state_changes::{
    apply_resize_end_to_selected, apply_resize_start_to_selected,
};
use crate::{EditState, Editor};
use lumino_editor_state::LineToolInteraction;
use lumino_editor_state::ShapeToolInteraction;
use lumino_message::Tool;

impl Editor {
    /// 处理鼠标释放事件
    pub(crate) fn handle_released(&mut self) {
        crate::puffin_profiler::released_handle();
        if self.release_round_corner() {
            return;
        }
        if self.release_slice_cut() {
            return;
        }

        // 框选结束：清除量化锚点（框选过程中的临时状态，任何一条松手路径都不再需要）。
        // 放在最前，保证 i2m / 文字 / 曲线等提前返回的分支同样被清理。
        self.selection_ui.set_marquee_anchor(None);
        let edit_state = std::mem::take(&mut self.editor_state.interaction.edit_state);

        // DRAW-049：Bake 放置态 B 阶段拉框收口
        if self.bake_marquee_active() {
            self.handle_bake_marquee_released(edit_state);
            return;
        }

        // 图形选中工具：结束拖动（提交移动）或结束空白拉框（按框命中选中）
        // 020：编辑图形的锚点拖拽同样在这里收口（`finish_shape_edit`），否则松手
        // 不提交、下一次按下会覆盖掉未完成的手柄状态。
        if self.is_shape_editing()
            || self.editor_state.shape_select.is_dragging()
            || self.editor_state.shape_select.is_marqueeing()
        {
            self.handle_shape_select_released();
            return;
        }

        // 图片转 MIDI 放置模式：框选完成/移动拉伸结束优先处理
        if self.editor_state.image_to_midi.is_active() {
            self.handle_i2m_released(edit_state);
            return;
        }

        // 以下手势按原判定顺序逐个尝试：每个方法把「进入条件 + 收口处理」成对封装，
        // 命中即返回，未命中不产生任何副作用。
        if self.release_curve_line_tool()
            || self.release_text_move()
            || self.release_shape_tool()
            || self.release_text_marquee(&edit_state)
            || self.release_brush_stroke()
        {
            return;
        }

        self.release_edit_state(edit_state);
    }

    /// DRAW-044：柔化拖拽收口（一次拖拽 = 一次 pending 撤销）
    fn release_round_corner(&mut self) -> bool {
        self.round_corners_active() && self.round_corner_released()
    }

    /// DRAW-025：切割松手 = 按拖出的线切分（失败保留布防）
    fn release_slice_cut(&mut self) -> bool {
        if self
            .shape_tools
            .slice_cut
            .is_some_and(|state| state.dragging)
        {
            self.slice_cut_released();
            return true;
        }
        false
    }

    /// 曲线工具直线模式：结束锚点/连线拖动
    fn release_curve_line_tool(&mut self) -> bool {
        if self.editor_state.tool == Tool::Curve
            && self.editor_state.line_tool.interaction != LineToolInteraction::None
        {
            self.handle_line_tool_released();
            return true;
        }
        false
    }

    /// 文字工具：已放置框的拖拽移动结束 → 清除拖拽临时状态
    fn release_text_move(&mut self) -> bool {
        if self.editor_state.tool == Tool::Text && self.editor_state.text_tool.is_dragging() {
            self.editor_state.text_tool.end_move();
            return true;
        }
        false
    }

    /// 形状工具：结束拖拽拉框 → 生成待确认图形（等待 √ 确认）
    fn release_shape_tool(&mut self) -> bool {
        if self.editor_state.tool == Tool::Shape
            && self.editor_state.shape_tool.interaction != ShapeToolInteraction::None
        {
            self.handle_shape_tool_released();
            return true;
        }
        false
    }

    /// 文字工具：拖拽拉框完成 → 吸附网格并进入编辑态
    /// Conductor 音轨（track 0）整工具不可用：即使存在 Selecting 也不进入编辑态
    fn release_text_marquee(&mut self, edit_state: &EditState) -> bool {
        if self.editor_state.tool == Tool::Text
            && self.text_tool_allowed()
            && let EditState::Selecting {
                start_tick,
                current_tick,
                start_key,
                current_key,
                ..
            } = edit_state
        {
            self.editor_state.text_tool.set_drag(
                *start_tick,
                *current_tick,
                *start_key,
                *current_key,
            );
            self.editor_state
                .text_tool
                .begin_editing(self.editor_state.view.snap_precision);
            return true;
        }
        false
    }

    /// 画刷笔画结束：进入待确认状态（√ 才生成音符）
    fn release_brush_stroke(&mut self) -> bool {
        if self.editor_state.brush_tool.is_active() {
            self.finish_brush_stroke();
            return true;
        }
        false
    }

    /// 按编辑状态收口音符级手势：match 臂序即语义，不可调整
    fn release_edit_state(&mut self, edit_state: EditState) {
        match edit_state {
            EditState::Selecting { .. } => self.commit_selecting(),
            EditState::Drawing {
                start_tick,
                key,
                current_tick,
            } => self.finish_drawing(start_tick, key, current_tick),
            EditState::PendingDrag { .. } => {}
            EditState::Dragging {
                note_index,
                drag_state,
                ..
            } => self.commit_dragging(note_index, drag_state),
            EditState::ResizingStart {
                note_index,
                original_tick,
                original_length,
            } => self.commit_resize_start(note_index, original_tick, original_length),
            EditState::ResizingEnd {
                note_index,
                original_length,
            } => self.commit_resize_end(note_index, original_length),
            EditState::DraggingSelection { drag_state } => {
                self.commit_dragging_selection(drag_state);
            }
            EditState::DraggingSelectionCopy { drag_state } => {
                self.commit_copy_dragging(drag_state);
            }
            EditState::ResizingSelectionStart {
                origin_tick,
                last_tick,
            } => self.commit_resize_selection_start(origin_tick, last_tick),
            EditState::ResizingSelectionEnd {
                origin_tick,
                last_tick,
            } => self.commit_resize_selection_end(origin_tick, last_tick),
            _ => {}
        }
    }

    /// 框选结束：橡皮擦删除已选音符，其余模式广播本地选择变更
    fn commit_selecting(&mut self) {
        if self.editor_state.tool == Tool::Eraser || self.editor_state.tool == Tool::DrawEraser {
            // 框选过程中 update_selection 已维护好 selected_notes，
            // 直接复用，避免重复线性扫描。
            self.delete_selected_notes();
        } else {
            tracing::debug!(
                "框选结束，选中 {} 个音符",
                self.editor_state.interaction.selected_notes.len()
            );
            // 广播本地选择变更（供协作对端高亮 + first-writer-wins 冲突判定）。
            // 协作未连接时跳过：消费端 `is_connected` 会短路丢弃，而指纹构建
            // （百万级选中 ~100ms）纯浪费。
            if self.editor_state.data.collab_sync_enabled() {
                let fps = self.selected_fingerprints();
                self.transient.emit_local_selection_changed(true, &fps);
            }
        }
    }

    /// 单音符拖动松手：一次性应用 delta（ghost 方案）
    fn commit_dragging(&mut self, note_index: usize, drag_state: lumino_editor_state::DragState) {
        // ghost 方案：松手时一次性应用 delta 到 notes
        if self.finalize_dragging(note_index, drag_state) {
            self.mark_notes_changed();
        }
    }

    /// 调整音符起始点：仅在 tick / 长度实际变化时提交
    fn commit_resize_start(&mut self, note_index: usize, original_tick: f32, original_length: f32) {
        // 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick/length f32）
        if let Some(note) = self.editor_state.data.get_note_view(note_index)
            && (note.tick != original_tick || note.length != original_length)
        {
            self.mark_notes_changed();
        }
    }

    /// 调整音符结束点：仅在长度实际变化时提交
    fn commit_resize_end(&mut self, note_index: usize, original_length: f32) {
        if let Some(note) = self.editor_state.data.get_note_view(note_index)
            && note.length != original_length
        {
            self.mark_notes_changed();
        }
    }

    /// 批量拖动松手：delta 为零不保存；否则按飞行状态串行化 / 累积 / 新建 pending
    fn commit_dragging_selection(&mut self, drag_state: lumino_editor_state::DragState) {
        crate::puffin_profiler::released_dragging_selection();
        // 松手：停止批量拖动预览序列（剩余未弹出的试听音符作废）
        self.editor_state.interaction.clear_preview_sequence();
        // ghost 方案（延迟提交）：松手不 apply，保存到 pending_drag_state。
        // 用户点击空白处取消框选时才 apply（commit_pending_drag）。
        //
        // 累积模式：如果已有 pending_drag_state，新 delta 叠加到 pending.delta。
        // 渲染时 ghost 位置 = note + pending.delta + drag_state.delta。
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: 批量拖动 delta 为零，不保存 pending");
        } else if self.editor_state.data.has_pending_commit() {
            // 飞行串行化：已有未完成的异步提交时不允许直接累积到飞行中的 pending
            // （飞行 ops 已按旧 delta 快照，累积会污染 pending 导致视觉与内存分叉）。
            // 先等待落盘（选中按新值重选、pending 清空），再按当前选中新基准存储当前拖动。
            let cur_dt = drag_state.delta_tick;
            let cur_dk = drag_state.delta_key;
            let cur_init_tick = drag_state.initial_tick;
            let cur_init_key = drag_state.initial_key;
            self.drain_async_commit();
            let note_count = self.editor_state.data.current_track_note_count();
            let mut fresh = lumino_editor_state::DragState::from_indices(
                self.get_selected_indices(),
                note_count,
                cur_init_tick,
                cur_init_key,
            );
            fresh.set_delta(cur_dt, cur_dk);
            tracing::debug!(
                "Editor: 飞行串行化后新基准 pending - delta=({}, {})",
                cur_dt,
                cur_dk
            );
            self.transient.pending_drag = Some(fresh);
        } else if let Some(mut pending) = self.transient.pending_drag.take() {
            pending.delta_tick = pending.delta_tick.saturating_add(drag_state.delta_tick);
            pending.delta_key = pending.delta_key.saturating_add(drag_state.delta_key);
            tracing::debug!(
                "Editor: 累积 pending 拖动 - 累积 delta=({}, {})",
                pending.delta_tick,
                pending.delta_key
            );
            self.transient.pending_drag = Some(pending);
        } else {
            tracing::debug!(
                "Editor: 保存 pending 拖动 - delta=({}, {})",
                drag_state.delta_tick,
                drag_state.delta_key
            );
            self.transient.pending_drag = Some(drag_state);
        }
        // 保留 selected_notes 不清空（pending 状态下仍显示框选）
        // edit_state 切换到 Idle（std::mem::take 已处理）
        // 不调用 mark_notes_changed（document 未变）
    }

    /// 批量复制拖动松手：delta 非零则立即把副本写入内存层
    fn commit_copy_dragging(&mut self, drag_state: lumino_editor_state::DragState) {
        crate::puffin_profiler::released_dragging_selection();
        // 松手：停止批量拖动预览序列（复制拖动同样有发声反馈）
        self.editor_state.interaction.clear_preview_sequence();
        // 复制模式（ghost 方案，**松手即提交**）：副本立即写入内存层，
        // 不再延迟到点击空白处（BUG 修复：复制后滚动/切换视图副本
        // "消失"、内存无数据——用户看到副本放置成功，必须内存同步
        // 真实化，否则副本只存在于 UI 层 pending，未提交即丢失）。
        //
        // 连续复制（复制下一份）：副本已真实化并**只选中副本**
        // （commit_pending_copy 按参数重选），第二次 Ctrl+拖动副本框时
        // `DragState.selected = 副本`，新副本从副本位置继续偏移——
        // 语义与旧「pending 累积」一致且更直观（每次复制独立可撤销）。
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: 复制拖动 delta 为零，取消复制");
        } else {
            self.transient.pending_copy = Some(drag_state);
            self.commit_pending_copy();
        }
        // 不调用 mark_notes_changed（commit_pending_copy 内部已标记）
    }

    /// 批量拉伸起始边：先改音符，再恢复排序不变式并按身份重选
    fn commit_resize_selection_start(&mut self, origin_tick: f32, last_tick: f32) {
        self.selection_ui.set_bounds(None);
        let delta_tick = last_tick - origin_tick;
        if delta_tick != 0.0 {
            let selected = self.get_selected_indices();
            // 主选择漂移防护：拉伸左边缘可越过未选中音符 → 索引位移
            let identity = self.capture_selection_identity();
            if let Some(track) = self
                .editor_state
                .data
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(self.editor_state.data.current_track))
            {
                apply_resize_start_to_selected(
                    delta_tick,
                    self.editor_state.view.snap_precision,
                    &selected,
                    track,
                );
            }
            // 越过邻居 → 恢复「按 start_tick 升序」不变式（二分查询依赖）；
            // 重排时按受影响闭区间增量更新（替代全量重建）
            if !self
                .editor_state
                .data
                .restore_current_track_sorted_incremental(&selected)
            {
                self.editor_state.data.record_update_ranges(&selected);
            }
            self.remap_selection_by_identity(&identity);
        }
        self.mark_notes_changed();
    }

    /// 批量拉伸结束边：先改音符长度，再按受影响区间记录更新
    fn commit_resize_selection_end(&mut self, origin_tick: f32, last_tick: f32) {
        self.selection_ui.set_bounds(None);
        let delta_tick = last_tick - origin_tick;
        if delta_tick != 0.0 {
            let selected = self.get_selected_indices();
            if let Some(track) = self
                .editor_state
                .data
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(self.editor_state.data.current_track))
            {
                apply_resize_end_to_selected(
                    delta_tick,
                    self.editor_state.view.snap_precision,
                    &selected,
                    track,
                );
            }
            self.editor_state.data.record_update_ranges(&selected);
        }
        self.mark_notes_changed();
    }

    /// 完成绘制新音符
    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let v = &self.editor_state.view;
        // 默认音符长度优先使用上次放置的长度，其次使用精度设置的默认长度
        let effective_default_length = v.last_note_length.unwrap_or(v.default_note_length);
        // 主选择漂移防护：插入位移既有索引，先捕获选中身份
        let selection_identity = self.capture_selection_identity();
        if let Some(note) = self.editor_state.data.finish_drawing(
            start_tick,
            key,
            current_tick,
            v.snap_precision,
            effective_default_length,
        ) {
            self.remap_selection_by_identity(&selection_identity);
            // 保存本次放置的音符长度，作为下次预览和放置的默认长度
            self.editor_state.view.set_last_note_length(note.length);
            self.emit_note_added_event(&note);
            self.mark_notes_changed();
        }
    }
}
