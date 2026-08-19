//! 曲线工具路径命中测试与坐标转换
//!
//! 包含：锚点/控制柄/曲线段命中测试（跨全部路径）、放置吸附、
//! 逻辑坐标 ↔ 屏幕坐标转换（key 支持 f32 自由值）。

use super::LineToolHit;
use super::geom;
use crate::Editor;
use iced_core::Point;
use lumino_editor_state::HandleSide;

/// 锚点命中半径（像素）
pub(super) const ANCHOR_HIT_RADIUS: f32 = 12.0;
/// 控制柄命中半径（像素）
pub(super) const HANDLE_HIT_RADIUS: f32 = 10.0;
/// 控制柄与锚点重合判定阈值（像素）：重合时柄不参与命中（点击拖动锚点）
pub(super) const HANDLE_COINCIDE_THRESHOLD_PX: f32 = 6.0;
/// 曲线段命中阈值（像素）
pub(super) const LINE_HIT_THRESHOLD: f32 = 8.0;
/// 锚点吸附阈值（像素）：放置新锚点时，周围（阈值内）存在锚点则吸附到其位置
///
/// 吸附目标仅限锚点（圆点），不含控制柄（方块）。
pub(super) const ANCHOR_SNAP_THRESHOLD_PX: f32 = 20.0;
/// 拖动锚点磁吸阈值（像素）：拖动中目标位置周围（阈值内）存在**其他路径**
/// 锚点时自动吸附（支持闭合图形/端点对齐）
pub(super) const ANCHOR_MAGNET_THRESHOLD_PX: f32 = 16.0;

impl Editor {
    /// 路径命中测试（控制柄 > 锚点 > 曲线段，跨全部完整路径；仅路径完整时有效）
    pub fn line_tool_hit_test(&self, pos: Point) -> Option<LineToolHit> {
        let line = &self.editor_state.line_tool;
        // 1. 控制柄（首锚点 out、尾锚点 in、中间 in+out）
        //    柄与锚点重合（未弯曲）时不参与命中——此时点击拖动锚点
        for (pi, path) in line.paths.iter().enumerate() {
            if path.len() < 2 {
                continue;
            }
            for (ai, anchor) in path.iter().enumerate() {
                let ap = self.line_pos_screen_pos(anchor.pos);
                for side in line.visible_handle_sides(pi, ai) {
                    let h_abs = match side {
                        HandleSide::In => anchor.in_handle_abs(),
                        HandleSide::Out => anchor.out_handle_abs(),
                    };
                    let hp = self.line_pos_screen_pos(h_abs);
                    if (hp.x - ap.x).hypot(hp.y - ap.y) < HANDLE_COINCIDE_THRESHOLD_PX {
                        continue;
                    }
                    if (pos.x - hp.x).hypot(pos.y - hp.y) <= HANDLE_HIT_RADIUS {
                        return Some(LineToolHit::Handle {
                            path: pi,
                            anchor_idx: ai,
                            side,
                        });
                    }
                }
            }
        }
        // 2. 锚点
        for (pi, path) in line.paths.iter().enumerate() {
            if path.len() < 2 {
                continue;
            }
            for (ai, anchor) in path.iter().enumerate() {
                let ap = self.line_pos_screen_pos(anchor.pos);
                if (pos.x - ap.x).hypot(pos.y - ap.y) <= ANCHOR_HIT_RADIUS {
                    return Some(LineToolHit::Anchor { path: pi, idx: ai });
                }
            }
        }
        // 3. 曲线段（采样折线逼近）
        for (pi, path) in line.paths.iter().enumerate() {
            if path.len() < 2 {
                continue;
            }
            for (si, pair) in path.windows(2).enumerate() {
                let (a, b) = (pair[0], pair[1]);
                let pa = self.line_pos_screen_pos(a.pos);
                let p1 = self.line_pos_screen_pos(a.out_handle_abs());
                let p2 = self.line_pos_screen_pos(b.in_handle_abs());
                let pb = self.line_pos_screen_pos(b.pos);
                if geom::point_curve_distance(pos, pa, p1, p2, pb) <= LINE_HIT_THRESHOLD {
                    return Some(LineToolHit::Segment {
                        path: pi,
                        segment: si,
                    });
                }
            }
        }
        None
    }

    /// 放置吸附：返回距离 `raw`（屏幕距离）<= 阈值、且未被 `exclude` 排除的
    /// 最近锚点位置（跨全部路径）；无则返回 None。
    ///
    /// 吸附目标仅限锚点（圆点），不认控制柄（方块）。
    pub(super) fn snap_new_anchor(
        &self,
        raw: (f32, f32),
        exclude: &[(usize, usize)],
    ) -> Option<(f32, f32)> {
        let raw_screen = self.line_pos_screen_pos(raw);
        let mut best: Option<((f32, f32), f32)> = None;
        for (pi, path) in self.editor_state.line_tool.paths.iter().enumerate() {
            for (ai, anchor) in path.iter().enumerate() {
                if exclude.contains(&(pi, ai)) {
                    continue;
                }
                let ap = self.line_pos_screen_pos(anchor.pos);
                let dist = (ap.x - raw_screen.x).hypot(ap.y - raw_screen.y);
                if dist <= ANCHOR_SNAP_THRESHOLD_PX && best.is_none_or(|(_, d)| dist < d) {
                    best = Some((anchor.pos, dist));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// 拖动磁吸：返回 `target`（逻辑坐标）附近（屏幕距离 <= 阈值）最近的
    /// **其他路径**锚点位置；无则返回 None。
    ///
    /// 排除 `exclude_path` 同路径的全部锚点（避免拖动时被相邻锚点卡住
    /// 造成路径退化）；跨路径锚点参与磁吸（闭合图形/端点对齐）。
    pub(super) fn snap_anchor_drag(
        &self,
        target: (f32, f32),
        exclude_path: usize,
    ) -> Option<(f32, f32)> {
        let target_screen = self.line_pos_screen_pos(target);
        let mut best: Option<((f32, f32), f32)> = None;
        for (pi, path) in self.editor_state.line_tool.paths.iter().enumerate() {
            if pi == exclude_path {
                continue;
            }
            for anchor in path {
                let ap = self.line_pos_screen_pos(anchor.pos);
                let dist = (ap.x - target_screen.x).hypot(ap.y - target_screen.y);
                if dist <= ANCHOR_MAGNET_THRESHOLD_PX && best.is_none_or(|(_, d)| dist < d) {
                    best = Some((anchor.pos, dist));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// 锚点/控制柄屏幕位置（key 支持 f32 自由值）
    pub fn line_pos_screen_pos(&self, pos: (f32, f32)) -> Point {
        let v = &self.editor_state.view;
        let max_key_index = (v.visible_key_count - 1) as f32;
        Point::new(
            v.tick_to_x(pos.0),
            (max_key_index - pos.1) * v.zoom_y - v.scroll_y + v.ruler_height,
        )
    }

    /// y 坐标 → key（f32 原始值，不取整；中间锚点自由定位用）
    ///
    /// `pub(crate)`：moved.rs（interaction 父模块）直接调用
    pub(crate) fn raw_y_to_key(&self, y: f32) -> f32 {
        let v = &self.editor_state.view;
        let max_key_index = (v.visible_key_count - 1) as f32;
        max_key_index - (y - v.ruler_height + v.scroll_y) / v.zoom_y
    }
}
