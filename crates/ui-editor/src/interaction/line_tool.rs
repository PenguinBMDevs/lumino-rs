//! 曲线工具直线模式交互：两点拉直线 → √ 批量生成音符
//!
//! 交互流程：
//! - 第一次左键按下：设置起点锚点（snap tick, key）；
//! - 第二次左键按下：设置终点锚点，直线完整（显示 √ × 按钮）；
//! - 直线完整后：拖动锚点可独立移动；拖动连线整体平移；
//!   按下空白处重新开始（清空后设置新起点锚点）；
//! - √ 按钮：按直线经过的网格格点批量生成音符（Bresenham）；
//! - × 按钮：清空直线状态。
//!
//! 交互状态独立于 `EditState`（`LineToolInteraction`），不耦合音符选择机制。

use crate::{Editor, Note};
use iced_core::Point;
use lumino_editor_state::{LineAnchor, LineToolInteraction};
use lumino_note_core::history::CreateOp;

/// 锚点命中半径（像素）
const ANCHOR_HIT_RADIUS: f32 = 12.0;
/// 连线命中阈值（像素）
const LINE_HIT_THRESHOLD: f32 = 8.0;

/// 直线命中类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineHitType {
    /// 命中起点锚点
    AnchorStart,
    /// 命中终点锚点
    AnchorEnd,
    /// 命中连线
    Line,
}

impl Editor {
    /// 直线模式按下处理
    ///
    /// - 直线未完整：设置下一个锚点（起点 → 终点）；
    /// - 直线完整：优先锚点/连线命中（进入拖动），未命中则清空并从此点开始新直线。
    pub(super) fn handle_line_tool_pressed(&mut self, pos: Point, snapped_tick: f32, key: u16) {
        if !self.editor_state.line_tool.is_complete() {
            // 未完整：设置下一个锚点
            self.editor_state
                .line_tool
                .set_next_anchor(snapped_tick, key);
            return;
        }
        // 完整直线：先检测锚点/连线命中（可拖动）
        let hit = self.line_tool_hit_test(pos);
        let line = &mut self.editor_state.line_tool;
        if let Some(hit) = hit {
            let (Some(a), Some(b)) = (line.anchor_start, line.anchor_end) else {
                return;
            };
            line.drag_start_snap = (snapped_tick, key);
            match hit {
                LineHitType::AnchorStart => {
                    line.drag_anchor_orig = a;
                    line.interaction = LineToolInteraction::DraggingAnchorStart;
                }
                LineHitType::AnchorEnd => {
                    line.drag_anchor_orig = b;
                    line.interaction = LineToolInteraction::DraggingAnchorEnd;
                }
                LineHitType::Line => {
                    line.drag_line_orig_start = a;
                    line.drag_line_orig_end = b;
                    line.interaction = LineToolInteraction::DraggingLine;
                }
            }
        } else {
            // 按下空白处：清空并从此点开始新直线
            line.reset();
            line.anchor_start = Some((snapped_tick, key));
        }
    }

    /// 直线模式移动处理（增量式拖动，锚点始终落在网格上）
    ///
    /// 以按下时的吸附值为锚点累加增量（与 i2m 区域框拖动语义一致），
    /// 避免直接对锚点赋全局吸附值导致的抖动。
    pub(super) fn handle_line_tool_moved(&mut self, snapped_tick: f32, key: u16) {
        let max_key = self.editor_state.view.key_count.saturating_sub(1);
        let line = &mut self.editor_state.line_tool;
        let (start_tick, start_key) = line.drag_start_snap;
        let delta_tick = snapped_tick - start_tick;
        let delta_key = i32::from(key) - i32::from(start_key);
        match line.interaction {
            LineToolInteraction::DraggingAnchorStart => {
                line.anchor_start = Some(Self::offset_anchor(
                    line.drag_anchor_orig,
                    delta_tick,
                    delta_key,
                    max_key,
                ));
            }
            LineToolInteraction::DraggingAnchorEnd => {
                line.anchor_end = Some(Self::offset_anchor(
                    line.drag_anchor_orig,
                    delta_tick,
                    delta_key,
                    max_key,
                ));
            }
            LineToolInteraction::DraggingLine => {
                line.anchor_start = Some(Self::offset_anchor(
                    line.drag_line_orig_start,
                    delta_tick,
                    delta_key,
                    max_key,
                ));
                line.anchor_end = Some(Self::offset_anchor(
                    line.drag_line_orig_end,
                    delta_tick,
                    delta_key,
                    max_key,
                ));
            }
            LineToolInteraction::None => {}
        }
    }

    /// 直线模式释放处理：结束锚点/连线拖动
    pub(super) fn handle_line_tool_released(&mut self) {
        self.editor_state.line_tool.interaction = LineToolInteraction::None;
    }

    /// 按增量偏移锚点（tick 不小于 0，key 限制在琴键范围内）
    fn offset_anchor(
        orig: LineAnchor,
        delta_tick: f32,
        delta_key: i32,
        max_key: u16,
    ) -> LineAnchor {
        let tick = (orig.0 + delta_tick).max(0.0);
        let key = (i32::from(orig.1) + delta_key).clamp(0, i32::from(max_key)) as u16;
        (tick, key)
    }

    /// 锚点屏幕位置（key 格中心）
    pub fn line_anchor_screen_pos(&self, anchor: LineAnchor) -> Point {
        let view = &self.editor_state.view;
        Point::new(
            view.tick_to_x(anchor.0),
            view.key_to_y(anchor.1) + view.zoom_y * 0.5,
        )
    }

    /// 直线命中测试（锚点优先，其次连线；仅直线完整时有效）
    pub fn line_tool_hit_test(&self, pos: Point) -> Option<LineHitType> {
        let line = &self.editor_state.line_tool;
        let (a, b) = (line.anchor_start?, line.anchor_end?);
        for (anchor, hit) in [(a, LineHitType::AnchorStart), (b, LineHitType::AnchorEnd)] {
            let ap = self.line_anchor_screen_pos(anchor);
            let dist = (pos.x - ap.x).hypot(pos.y - ap.y);
            if dist <= ANCHOR_HIT_RADIUS {
                return Some(hit);
            }
        }
        let pa = self.line_anchor_screen_pos(a);
        let pb = self.line_anchor_screen_pos(b);
        if point_segment_distance(pos, pa, pb) <= LINE_HIT_THRESHOLD {
            return Some(LineHitType::Line);
        }
        None
    }

    /// 确认直线：按直线经过的网格格点批量生成音符（√ 按钮）
    ///
    /// 生成规则：每个格点一个音符，长度 = 当前吸附精度（首尾相接成连续旋律），
    /// 写入当前音轨并使用 `CreateOp` 操作日志（跨轨撤销/重做）。
    /// 成功后清空直线状态；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        let line = &self.editor_state.line_tool;
        let (Some(a), Some(b)) = (line.anchor_start, line.anchor_end) else {
            return false;
        };
        let snap = self.editor_state.view.snap_precision;
        let points = line_cell_points(a, b, snap);
        if points.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let mut create_ops = Vec::with_capacity(points.len());
        for (tick, key) in points {
            let note = Note::new(tick, key, snap);
            if self.editor_state.data.insert_note(track, note.clone()) {
                create_ops.push(CreateOp {
                    track_id: track as u32,
                    note: lumino_editor_state::note_to_event(note),
                });
            }
        }
        if create_ops.is_empty() {
            return false;
        }

        // 批量创建操作日志（撤销/重做）+ 标记当前轨变化
        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        // 清空直线状态并驱动渲染刷新
        self.editor_state.line_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消直线（× 按钮）
    pub(crate) fn cancel_line_tool(&mut self) {
        self.editor_state.line_tool.reset();
    }
}

/// 点到线段的最短距离
fn point_segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let len_sq = ab_x * ab_x + ab_y * ab_y;
    if len_sq <= f32::EPSILON {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * ab_x + (p.y - a.y) * ab_y) / len_sq).clamp(0.0, 1.0);
    let proj_x = a.x + t * ab_x;
    let proj_y = a.y + t * ab_y;
    (p.x - proj_x).hypot(p.y - proj_y)
}

/// Bresenham 直线算法：生成直线经过的所有网格格点
///
/// tick 方向按 `snap` 分格，key 方向每个 key 一格；
/// 结果按路径顺序排列（tick/key 单调，无重复格点）。
pub(crate) fn line_cell_points(a: LineAnchor, b: LineAnchor, snap: f32) -> Vec<LineAnchor> {
    let snap = snap.max(1.0);
    let x0 = (a.0 / snap).round() as i64;
    let y0 = i64::from(a.1);
    let x1 = (b.0 / snap).round() as i64;
    let y1 = i64::from(b.1);
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let mut points = Vec::with_capacity((dx + dy + 1) as usize);
    loop {
        points.push((x as f32 * snap, y as u16));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::seed_notes;
    use lumino_core::Tool;

    #[test]
    fn test_line_cell_points_horizontal() {
        let pts = line_cell_points((0.0, 60), (3840.0, 60), 1920.0);
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0], (0.0, 60));
        assert_eq!(pts[1], (1920.0, 60));
        assert_eq!(pts[2], (3840.0, 60));
    }

    #[test]
    fn test_line_cell_points_vertical() {
        let pts = line_cell_points((1920.0, 60), (1920.0, 64), 1920.0);
        assert_eq!(pts.len(), 5);
        assert_eq!(pts[0], (1920.0, 60));
        assert_eq!(pts[4], (1920.0, 64));
    }

    #[test]
    fn test_line_cell_points_diagonal() {
        // 45° 斜线：5 个格点（含端点）
        let pts = line_cell_points((0.0, 60), (1920.0, 64), 1920.0);
        assert_eq!(pts.len(), 5);
        assert_eq!(pts[0], (0.0, 60));
        assert_eq!(pts[4], (1920.0, 64));
    }

    #[test]
    fn test_line_cell_points_reverse_order() {
        // 反向（从右到左、低到高）同样覆盖全部格点
        let mut pts = line_cell_points((3840.0, 60), (0.0, 64), 1920.0);
        pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        pts.dedup();
        assert_eq!(pts.len(), 5, "反向直线应覆盖 5 个格点");
    }

    #[test]
    fn test_line_cell_points_single() {
        let pts = line_cell_points((0.0, 60), (0.0, 60), 1920.0);
        assert_eq!(pts, vec![(0.0, 60)]);
    }

    #[test]
    fn test_point_segment_distance() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(20.0, 0.0);
        // 线段上
        assert!((point_segment_distance(Point::new(10.0, 0.0), a, b)).abs() < 1e-6);
        // 线段上方
        assert!((point_segment_distance(Point::new(10.0, 5.0), a, b) - 5.0).abs() < 1e-6);
        // 端点之外：最近距离到端点
        assert!((point_segment_distance(Point::new(30.0, 0.0), a, b) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_confirm_line_creates_notes() {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        // 单一权威源：音符写入 document，测试需先构造（与 tests/test_helpers 一致）
        seed_notes(&mut editor, 1, 0, &[]);
        editor.editor_state.line_tool.anchor_start = Some((0.0, 60));
        editor.editor_state.line_tool.anchor_end = Some((1920.0, 64));
        assert!(editor.confirm_line_tool());
        // 45° 线：5 个格点 → 5 个音符
        assert_eq!(editor.editor_state.data.current_track_note_count(), 5);
        // 直线状态已清空
        assert!(editor.editor_state.line_tool.anchor_start.is_none());
    }

    #[test]
    fn test_confirm_line_incomplete_noop() {
        let mut editor = Editor::new();
        editor.editor_state.line_tool.anchor_start = Some((0.0, 60));
        assert!(!editor.confirm_line_tool());
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
        // 未完整时确认不改变状态
        assert!(editor.editor_state.line_tool.anchor_start.is_some());
    }

    #[test]
    fn test_cancel_line_clears() {
        let mut editor = Editor::new();
        editor.editor_state.line_tool.anchor_start = Some((0.0, 60));
        editor.editor_state.line_tool.anchor_end = Some((1920.0, 64));
        editor.cancel_line_tool();
        assert!(editor.editor_state.line_tool.anchor_start.is_none());
        assert!(editor.editor_state.line_tool.anchor_end.is_none());
    }

    #[test]
    fn test_line_tool_drag_anchor_moves_only_one() {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        {
            let line = &mut editor.editor_state.line_tool;
            line.anchor_start = Some((0.0, 60));
            line.anchor_end = Some((1920.0, 64));
        }
        let a_pos = editor.line_anchor_screen_pos((0.0, 60));

        // 按下起点锚点 → 进入 DraggingAnchorStart
        editor.handle_line_tool_pressed(a_pos, 0.0, 60);
        assert_eq!(
            editor.editor_state.line_tool.interaction,
            LineToolInteraction::DraggingAnchorStart
        );
        // 移动到 (+1920, +4)：仅起点锚点移动，终点锚点不动
        editor.handle_line_tool_moved(1920.0, 64);
        assert_eq!(
            editor.editor_state.line_tool.anchor_start,
            Some((1920.0, 64))
        );
        assert_eq!(editor.editor_state.line_tool.anchor_end, Some((1920.0, 64)));
        // 释放后结束拖动
        editor.handle_line_tool_released();
        assert_eq!(
            editor.editor_state.line_tool.interaction,
            LineToolInteraction::None
        );
    }

    #[test]
    fn test_line_tool_drag_line_translates_both_anchors() {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        {
            let line = &mut editor.editor_state.line_tool;
            line.anchor_start = Some((0.0, 60));
            line.anchor_end = Some((1920.0, 64));
        }
        let a_pos = editor.line_anchor_screen_pos((0.0, 60));
        let b_pos = editor.line_anchor_screen_pos((1920.0, 64));
        let mid = Point::new((a_pos.x + b_pos.x) * 0.5, (a_pos.y + b_pos.y) * 0.5);

        // 按下连线中点 → 进入 DraggingLine（连线只能平移）
        editor.handle_line_tool_pressed(mid, 0.0, 60);
        assert_eq!(
            editor.editor_state.line_tool.interaction,
            LineToolInteraction::DraggingLine
        );
        // 平移 (+1920, -4)：两个锚点同步偏移，相对位置不变
        editor.handle_line_tool_moved(1920.0, 56);
        assert_eq!(
            editor.editor_state.line_tool.anchor_start,
            Some((1920.0, 56))
        );
        assert_eq!(editor.editor_state.line_tool.anchor_end, Some((3840.0, 60)));
        editor.handle_line_tool_released();
    }

    #[test]
    fn test_line_tool_press_blank_restarts() {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        {
            let line = &mut editor.editor_state.line_tool;
            line.anchor_start = Some((0.0, 60));
            line.anchor_end = Some((1920.0, 64));
        }

        // 远处空白按下 → 清空旧直线并从该点开始新直线
        editor.handle_line_tool_pressed(Point::new(800.0, 500.0), 1920.0, 30);
        let line = &editor.editor_state.line_tool;
        assert!(line.anchor_end.is_none(), "旧终点锚点应被清空");
        assert_eq!(line.anchor_start, Some((1920.0, 30)));
    }

    #[test]
    fn test_line_tool_two_clicks_set_anchors() {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        // 第一次点击：设置起点锚点
        editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60);
        assert_eq!(editor.editor_state.line_tool.anchor_start, Some((0.0, 60)));
        assert!(!editor.editor_state.line_tool.is_complete());
        // 第二次点击：设置终点锚点，直线完整
        editor.handle_line_tool_pressed(Point::new(312.0, 24.0), 1920.0, 64);
        assert_eq!(editor.editor_state.line_tool.anchor_end, Some((1920.0, 64)));
        assert!(editor.editor_state.line_tool.is_complete());
    }
}
