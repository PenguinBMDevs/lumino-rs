//! 图形选中工具（音符画工具栏「鼠标工具」）交互 —— 点选已绘制图形
//!
//! 与「登记」两面一体：
//! - **登记**：曲线 / 形状 / 画刷工具 √ 确认生成音符后，把各自的矢量几何登记进
//!   `EditorState::shape_select`（见 `record_*` 方法），并绑定本次音符创建历史的
//!   分组 ID，供撤销/重做同步（见 `Editor::undo/redo`）。
//! - **点选**：鼠标工具（`Tool::ShapeSelect`）左键按下 —— 命中当前音轨**最上层**
//!   的可见图形则选中（高亮描边由 `grid::drawn_shape_box` 渲染），点空白取消选中。
//!
//! 命中判定（逻辑坐标 → 屏幕像素空间，容差统一为像素）：
//! - 形状工具图形：直接复用 `point_in_shape`（含 Shift 正图形约束与圆形内部判定）；
//! - 折线（曲线展平路径 / 画刷笔画）：点到各段的屏幕距离 ≤ [`HIT_TOLERANCE_PX`]。
//!
//! 图形是**叠加对象**（音符才是真正的文档内容），故点选不会改动 document、不入历史。

use lumino_editor_state::{DrawnShapeSource, shape_tool::point_in_shape};
use lumino_note_core::history::HistoryEntry;

use crate::Editor;
use crate::interaction::line_tool::geom::flatten_path;

/// 折线命中容差（屏幕像素）：点击位置到笔画折线的最短距离阈值
const HIT_TOLERANCE_PX: f32 = 6.0;

/// 点到线段的最短距离（同坐标系）
fn point_segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let abx = b.0 - a.0;
    let aby = b.1 - a.1;
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = a.0 + abx * t;
    let cy = a.1 + aby * t;
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

impl Editor {
    /// 最近一次音符创建历史的**分组 ID**（`push_note_create` 之后调用）。
    ///
    /// 图形对象以此与历史条目绑定：撤销该条目时同组图形一并隐藏，
    /// 重做时恢复。非 Create 条目（或栈顶非 Create）返回 `None`。
    pub(crate) fn last_create_group(&self) -> Option<u64> {
        match self.editor_state.data.history.undo_back() {
            Some(HistoryEntry::Create(entry)) => entry.group_id,
            _ => None,
        }
    }

    /// 登记一个图形对象（自动绑定当前历史分组）
    fn record_drawn_shape_on(&mut self, track: usize, source: DrawnShapeSource) {
        let group = self.last_create_group();
        self.editor_state.shape_select.add(track, group, source);
    }

    /// 登记形状工具的待确认图形（√ 确认后调用，须在 `clear_pending` 之前）
    ///
    /// 形状工具只写入当前音轨，故统一登记在当前音轨。
    pub(crate) fn record_shape_tool_shapes(&mut self) {
        let track = self.editor_state.data.current_track;
        let sources: Vec<DrawnShapeSource> = self
            .editor_state
            .shape_tool
            .shapes
            .iter()
            .map(|s| DrawnShapeSource::Shape {
                kind: s.kind,
                rect: s.rect,
                shift_constrained: s.shift_constrained,
                filled: s.filled,
            })
            .collect();
        for source in sources {
            self.record_drawn_shape_on(track, source);
        }
    }

    /// 登记曲线工具的待确认路径（√ 确认后调用，须在 `reset` 之前）
    ///
    /// 每条完整路径（≥ 2 锚点）展开为一条折线；不满 2 锚点的悬空点不入登记。
    pub(crate) fn record_line_tool_paths(&mut self) {
        let track = self.editor_state.data.current_track;
        let polylines: Vec<Vec<(f32, f32)>> = self
            .editor_state
            .line_tool
            .paths
            .iter()
            .filter(|p| p.len() >= 2)
            .map(|p| {
                flatten_path(p)
                    .into_iter()
                    .map(|(t, k)| (t as f32, k as f32))
                    .collect()
            })
            .collect();
        for points in polylines {
            self.record_drawn_shape_on(track, DrawnShapeSource::Polyline { points });
        }
    }

    /// 登记画刷工具的待确认笔画（√ 确认后调用，须在 `reset` 之前）
    ///
    /// 画刷按层把音符分配到多个音轨（见 `BrushConfig::track_for_level`），
    /// 图形对象以其**落笔基准轨**（`BrushStroke::base_track`）登记——即画笔起始所在轨，
    /// 保证「在哪个轨画的就在哪个轨选中」。
    pub(crate) fn record_brush_strokes(&mut self) {
        let fallback = self.editor_state.data.current_track;
        let strokes: Vec<(usize, Vec<(f32, f32)>)> = self
            .editor_state
            .brush_tool
            .strokes
            .iter()
            .filter(|s| !s.points.is_empty())
            .map(|s| (if s.base_track == 0 { fallback } else { s.base_track }, s.points.clone()))
            .collect();
        for (track, points) in strokes {
            self.record_drawn_shape_on(track, DrawnShapeSource::Polyline { points });
        }
    }

    /// 鼠标工具：左键按下 —— 命中当前音轨最上层可见图形则选中，否则取消选中
    pub(crate) fn handle_shape_select_pressed(&mut self, tick: f32, key: f32) {
        let hit = self.hit_test_drawn_shape(tick, key);
        self.editor_state.shape_select.select(hit);
        // 仅叠加层视觉变化：清网格缓存驱动重绘。
        // **不能**用 `mark_notes_changed()`——那会置 notes_changed 并重建空间索引
        // （O(N)，百万音符工程下每次点选都白跑一遍），而 document 并未改动。
        // 与远端选中变更（`apply_remote_selection`）同一策略。
        self.grid_cache.clear();
    }

    /// 命中当前音轨**最上层**（后登记优先）的可见图形，返回其 ID
    pub(crate) fn hit_test_drawn_shape(&self, tick: f32, key: f32) -> Option<u64> {
        let track = self.editor_state.data.current_track;
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        let probe = self.line_pos_screen_pos((tick, key));

        // 逆序遍历：最后登记的图形画在最上层，命中也应优先
        for shape in self.editor_state.shape_select.visible_on(track).rev() {
            let hit = match &shape.source {
                DrawnShapeSource::Shape {
                    kind,
                    rect,
                    shift_constrained,
                    ..
                } => point_in_shape(
                    *kind,
                    *rect,
                    *shift_constrained,
                    px_per_tick,
                    px_per_key,
                    tick,
                    key,
                ),
                DrawnShapeSource::Polyline { points } => {
                    polyline_hit(self, probe, points)
                }
            };
            if hit {
                return Some(shape.id);
            }
        }
        None
    }
}

/// 折线命中：探测点到折线各段的屏幕距离是否 ≤ 容差
fn polyline_hit(editor: &Editor, probe: iced_core::Point, points: &[(f32, f32)]) -> bool {
    if points.is_empty() {
        return false;
    }
    let screen: Vec<(f32, f32)> = points
        .iter()
        .map(|&(t, k)| {
            let p = editor.line_pos_screen_pos((t, k));
            (p.x, p.y)
        })
        .collect();
    let probe = (probe.x, probe.y);
    if screen.len() == 1 {
        let d = ((probe.0 - screen[0].0).powi(2) + (probe.1 - screen[0].1).powi(2)).sqrt();
        return d <= HIT_TOLERANCE_PX;
    }
    screen
        .windows(2)
        .any(|seg| point_segment_distance(probe, seg[0], seg[1]) <= HIT_TOLERANCE_PX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::seed_notes;
    use lumino_editor_state::{BezierAnchor, BrushStroke, ShapeKind};
    use lumino_message::Tool;

    /// 构造非 Conductor 轨（track 1）编辑器，吸附精度 = 1
    fn test_editor() -> Editor {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        editor.editor_state.view.snap_precision = 1.0;
        editor
    }

    /// 拉出并确认一个矩形轮廓，返回编辑器
    fn draw_and_confirm_rect() -> Editor {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool(), "矩形应生成音符");
        editor
    }

    #[test]
    fn test_confirm_shape_registers_drawn_shape() {
        let editor = draw_and_confirm_rect();
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1, "确认后应登记 1 个图形对象");
        let shape = &st.shapes()[0];
        assert_eq!(shape.track, 1, "登记在当前音轨");
        assert!(shape.group.is_some(), "应绑定音符创建历史分组");
        match &shape.source {
            DrawnShapeSource::Shape {
                kind,
                rect,
                shift_constrained,
                filled,
            } => {
                assert_eq!(*kind, ShapeKind::Rectangle);
                assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0));
                assert!(!shift_constrained && !filled);
            }
            other => panic!("期望 Shape 几何，实际 {other:?}"),
        }
    }

    #[test]
    fn test_mouse_tool_selects_inside_and_deselects_outside() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 矩形内部（0..4 × 60..64）→ 选中
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(
            editor.editor_state.shape_select.selected().is_some(),
            "点在图形内部应选中"
        );
        // 远处空白 → 取消选中
        editor.handle_shape_select_pressed(900.0, 10.0);
        assert_eq!(
            editor.editor_state.shape_select.selected(),
            None,
            "点空白应取消选中"
        );
    }

    #[test]
    fn test_circle_hit_uses_ellipse_interior() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Circle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());
        editor.set_tool(Tool::ShapeSelect);
        // 圆心命中
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(editor.editor_state.shape_select.selected().is_some());
        // 外接框角（椭圆之外）不命中
        editor.handle_shape_select_pressed(0.05, 60.05);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_confirm_curve_registers_polyline() {
        let mut editor = test_editor();
        editor.editor_state.line_tool.paths = vec![vec![
            BezierAnchor::new((0.0, 60.0)),
            BezierAnchor::new((240.0, 64.0)),
        ]];
        editor.editor_state.line_tool.recompute_auto_handles();
        assert!(editor.confirm_line_tool(), "曲线应生成音符");
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1, "一条路径 = 一个图形对象");
        match &st.shapes()[0].source {
            DrawnShapeSource::Polyline { points } => {
                assert!(points.len() >= 2, "展平折线至少 2 点，实际 {}", points.len());
            }
            other => panic!("期望 Polyline 几何，实际 {other:?}"),
        }
    }

    #[test]
    fn test_undo_hides_shape_and_redo_restores() {
        let mut editor = draw_and_confirm_rect();
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
        assert!(editor.undo(), "撤销音符创建应成功");
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            0,
            "撤销后同组图形应隐藏（避免幽灵描边）"
        );
        assert!(editor.redo(), "重做应成功");
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            1,
            "重做后图形应恢复可见"
        );
    }

    #[test]
    fn test_hidden_shape_cannot_be_hit() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(editor.editor_state.shape_select.selected().is_some());
        assert!(editor.undo());
        // 撤销后原位置不再是可命中图形
        assert_eq!(editor.hit_test_drawn_shape(2.0, 62.0), None);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_other_track_shape_not_hit() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 切到另一音轨：图形登记在 track 1，当前轨 2 不应命中
        editor.editor_state.data.current_track = 2;
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_record_brush_strokes_uses_base_track() {
        let mut editor = test_editor();
        editor.editor_state.brush_tool.strokes.push(BrushStroke {
            points: vec![(0.0, 60.0), (4.0, 62.0)],
            base_track: 1,
        });
        editor.record_brush_strokes();
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1);
        assert_eq!(st.shapes()[0].track, 1, "应登记在落笔基准轨");
        assert!(matches!(
            st.shapes()[0].source,
            DrawnShapeSource::Polyline { .. }
        ));
    }

    #[test]
    fn test_reset_clears_registry() {
        let mut editor = draw_and_confirm_rect();
        assert!(!editor.editor_state.shape_select.is_empty());
        editor.editor_state.reset();
        assert!(editor.editor_state.shape_select.is_empty(), "reset 应清空图形注册表");
    }

    #[test]
    fn test_point_segment_distance_basics() {
        // 垂足落在线段内
        let d = point_segment_distance((5.0, 3.0), (0.0, 0.0), (10.0, 0.0));
        assert!((d - 3.0).abs() < 1e-4, "期望 3.0，实际 {d}");
        // 落在线段外 → 取端点距离
        let d = point_segment_distance((14.0, 0.0), (0.0, 0.0), (10.0, 0.0));
        assert!((d - 4.0).abs() < 1e-4, "期望 4.0，实际 {d}");
    }
}

