//! 形状工具拖拽绘制：矩形/圆/三角 拉出 → √ 批量确认生成音符
//!
//! 与 `line_tool` 同构（拖拽拉框 → 实时预览 → 确认生成音符），但纯拉框范式：
//! 形状在 √ 确认前为临时叠加，确认后「固化」为音符（每格一个，长度 = 吸附精度），
//! 与 `confirm_line_tool` 完全一致。填充桶（`fill_enabled`）决定确认时是否生成图形内部音符。

use std::collections::HashSet;

use lumino_editor_state::shape_tool::point_in_shape;
use lumino_note_core::history::CreateOp;

use crate::{Editor, Note};

impl Editor {
    /// 形状工具：左键按下 —— 开始拖拽拉框
    ///
    /// - Conductor 轨道（track 0）：整工具不可用，直接返回；
    /// - 填充桶开启且点击命中某待确认图形内部：标记该图形为「已填充」
    ///   （支持用填充桶填充已拉出的图案），不开始新拖拽；
    /// - 否则开始拖拽拉框。
    pub(crate) fn handle_shape_tool_pressed(
        &mut self,
        snapped_tick: f32,
        key: u16,
        _shift: bool,
    ) {
        // Conductor 音轨：形状工具不可用
        if self.editor_state.data.current_track == 0 {
            return;
        }
        // 填充桶：点击待确认图形内部 → 标记填充
        if self.editor_state.shape_tool.fill_enabled {
            if let Some(idx) = self.shape_hit_test(snapped_tick, key as f32) {
                self.editor_state.shape_tool.shapes[idx].filled = true;
                self.mark_notes_changed();
                return;
            }
        }
        // 正常：开始拖拽拉框
        self.editor_state
            .shape_tool
            .begin_drag((snapped_tick, key as f32));
    }

    /// 形状工具：拖拽移动 —— 更新当前点（实时预览）
    pub(crate) fn handle_shape_tool_moved(&mut self, snapped_tick: f32, key: f32) {
        self.editor_state
            .shape_tool
            .update_drag((snapped_tick, key));
    }

    /// 形状工具：左键释放 —— 结束拖拽，生成待确认图形
    pub(crate) fn handle_shape_tool_released(&mut self) {
        let snap = self.editor_state.view.snap_precision;
        let shift = self.shift_pressed();
        if self
            .editor_state
            .shape_tool
            .end_drag(snap, shift)
            .is_some()
        {
            self.mark_notes_changed();
        }
    }

    /// 形状工具：确认（√）—— 把所有待确认图形转成音符
    ///
    /// 轮廓图形只生成边界格音符；填充图形额外生成内部格音符。
    /// 与 `confirm_line_tool` 同构：先整体去重，再批量插入并写入历史。
    pub(crate) fn confirm_shape_tool(&mut self) -> bool {
        // Conductor 音轨：形状不可用
        if self.editor_state.data.current_track == 0 {
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        let snap_key = snap.max(1.0);

        let mut points: Vec<(f32, u16)> = Vec::new();
        for shape in &self.editor_state.shape_tool.shapes {
            let cells = lumino_editor_state::shape_tool::shape_cells(
                shape.kind,
                shape.rect,
                shape.shift_constrained,
                shape.filled,
                snap,
            );
            points.extend(cells);
        }
        // 整体去重（跨多个图形 + 图形内部/边界可能重合）
        let mut seen: HashSet<(i64, u16)> = HashSet::new();
        points.retain(|p| seen.insert(((p.0 / snap_key).round() as i64, p.1)));

        if points.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let mut create_ops: Vec<CreateOp> = Vec::with_capacity(points.len());
        for (tick, key) in points {
            let note = Note::new(tick, key, snap);
            if self
                .editor_state
                .data
                .insert_note(track, note.clone())
            {
                create_ops.push(CreateOp {
                    track_id: track as u32,
                    note: lumino_editor_state::note_to_event(note),
                });
            }
        }
        if create_ops.is_empty() {
            return false;
        }

        self.editor_state
            .data
            .history
            .push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        self.editor_state.shape_tool.clear_pending();
        self.mark_notes_changed();
        true
    }

    /// 形状工具：取消（×）—— 清空所有待确认图形（保留图形类型）
    pub(crate) fn cancel_shape_tool(&mut self) {
        self.editor_state.shape_tool.clear_pending();
        self.mark_notes_changed();
    }

    /// 命中待确认图形内部（用于填充桶点击填充），返回图形索引
    fn shape_hit_test(&self, tick: f32, key: f32) -> Option<usize> {
        for (i, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            if point_in_shape(shape.kind, shape.rect, shape.shift_constrained, tick, key) {
                return Some(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::seed_notes;
    use lumino_editor_state::ShapeKind;

    /// 构造一个非 Conductor 轨（track 1，空）、吸附精度 = 1 的编辑器
    fn test_editor() -> Editor {
        let mut editor = Editor::new();
        // 初始化 document 与 track 1（形状工具在 track 0 不可用），初始无音符
        seed_notes(&mut editor, 2, 1, &[]);
        // 吸附精度 1 tick，使格点对齐整数 tick / key
        editor.editor_state.view.snap_precision = 1.0;
        editor
    }

    #[test]
    fn test_rectangle_outline_produces_notes() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        // 拖出 0..4 × key 60..64 的矩形轮廓
        editor.handle_shape_tool_pressed(0.0, 60, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.editor_state.shape_tool.has_pending());
        // 拖拽过短校验：外接框应被规范化记录
        assert_eq!(
            editor.editor_state.shape_tool.shapes[0].rect,
            (0.0, 60.0, 4.0, 64.0)
        );
        let ok = editor.confirm_shape_tool();
        assert!(ok);
        // 轮廓 = 5×5 - 3×3 = 16 格
        assert_eq!(editor.editor_state.data.current_track_note_count(), 16);
        // 确认后待确认列表清空
        assert!(!editor.editor_state.shape_tool.has_pending());
    }

    #[test]
    fn test_filled_rectangle_produces_interior_notes() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = true;
        editor.handle_shape_tool_pressed(0.0, 60, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        let ok = editor.confirm_shape_tool();
        assert!(ok);
        // 填充矩形 = 5×5 = 25 格
        assert_eq!(editor.editor_state.data.current_track_note_count(), 25);
    }

    #[test]
    fn test_cancel_clears_pending() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.handle_shape_tool_pressed(0.0, 60, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.editor_state.shape_tool.has_pending());
        editor.cancel_shape_tool();
        assert!(!editor.editor_state.shape_tool.has_pending());
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    }

    #[test]
    fn test_conductor_track_rejects_shape() {
        let mut editor = test_editor();
        editor.editor_state.data.current_track = 0;
        editor.set_shape(ShapeKind::Rectangle);
        editor.handle_shape_tool_pressed(0.0, 60, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        // Conductor 轨道（track 0）整工具不可用，不应开始拖拽
        assert!(!editor.editor_state.shape_tool.has_pending());
    }

    #[test]
    fn test_fill_bucket_marks_existing_shape() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        // 先拉出轮廓（填充桶关闭）
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(!editor.editor_state.shape_tool.shapes[0].filled);
        // 开启填充桶并点选图形内部（命中）→ 标记填充
        editor.editor_state.shape_tool.fill_enabled = true;
        editor.handle_shape_tool_pressed(2.0, 62, false);
        assert!(editor.editor_state.shape_tool.shapes[0].filled);
    }
}

