//! 曲线工具贝塞尔路径交互：两点拉线 → 插入锚点弯曲 → √ 批量生成音符
//!
//! 交互流程：
//! - 前两次左键按下：设置首尾端点（tick 吸附、key 整数格）；
//! - 路径完整后：
//!   - 点击曲线段（原地松开）：在该段中间插入锚点（**不吸附网格**）；
//!   - 按下曲线段 + 拖动（超过 4px）：整条路径平移；
//!   - 拖动锚点：端点吸附网格、中间锚点自由移动；
//!   - 拖动控制柄：自由弯曲对应贝塞尔段；
//!   - 双击中间锚点：删除；
//!   - 空白处按下：清空并开始新路径；
//! - √ 按钮：按曲线经过的网格格点批量生成音符（贝塞尔离散化，见 `geom`）；
//! - × 按钮：清空路径状态。
//!
//! 交互状态独立于 `EditState`（`LineToolInteraction`），不耦合音符选择机制。
//! 纯几何算法（贝塞尔求值/距离/格点离散化）在 `line_tool/geom.rs`。

mod geom;

#[cfg(test)]
mod tests;

use crate::{Editor, Note};
use iced_core::Point;
use lumino_editor_state::{HandleSide, LineToolInteraction};
use lumino_note_core::history::CreateOp;

/// 锚点命中半径（像素）
const ANCHOR_HIT_RADIUS: f32 = 12.0;
/// 控制柄命中半径（像素）
const HANDLE_HIT_RADIUS: f32 = 10.0;
/// 控制柄与锚点重合判定阈值（像素）：重合时柄不参与命中（点击拖动锚点）
const HANDLE_COINCIDE_THRESHOLD_PX: f32 = 6.0;
/// 曲线段命中阈值（像素）
const LINE_HIT_THRESHOLD: f32 = 8.0;
/// 曲线段按下 → 拖动判定阈值（像素）：未超阈值松开视为点击插入锚点
const PRESS_DRAG_THRESHOLD_PX: f32 = 4.0;

/// 路径命中类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineToolHit {
    /// 命中指定锚点
    Anchor(usize),
    /// 命中控制柄
    Handle { anchor_idx: usize, side: HandleSide },
    /// 命中曲线段（索引 = anchors[i] → anchors[i+1]）
    Segment(usize),
}

impl Editor {
    /// 路径按下处理
    ///
    /// - 未完整：设置下一个端点（调用方已吸附 tick，key 为整数格）；
    /// - 完整：按 控制柄 > 锚点 > 曲线段 > 空白 的优先级分发。
    pub(super) fn handle_line_tool_pressed(
        &mut self,
        pos: Point,
        snapped_tick: f32,
        snapped_key: f32,
    ) {
        let raw = (self.x_to_tick(pos.x), self.raw_y_to_key(pos.y));
        // 未完整：设置端点
        if !self.editor_state.line_tool.is_complete() {
            self.editor_state
                .line_tool
                .push_anchor((snapped_tick, snapped_key));
            return;
        }
        // 完整路径：命中分发
        let hit = self.line_tool_hit_test(pos);
        let line = &mut self.editor_state.line_tool;
        match hit {
            Some(LineToolHit::Handle { anchor_idx, side }) => {
                let anchor = &line.anchors[anchor_idx];
                line.drag_handle_orig = match side {
                    HandleSide::In => anchor.in_handle,
                    HandleSide::Out => anchor.out_handle,
                };
                line.drag_start_raw = raw;
                line.drag_confirmed = true;
                line.interaction = LineToolInteraction::DraggingHandle { anchor_idx, side };
            }
            Some(LineToolHit::Anchor(idx)) => {
                line.drag_start_snap = (snapped_tick, snapped_key);
                line.drag_start_raw = raw;
                line.drag_anchor_orig = line.anchors[idx];
                line.drag_confirmed = true;
                line.interaction = LineToolInteraction::DraggingAnchor(idx);
            }
            Some(LineToolHit::Segment(segment)) => {
                // 按下待定：移动超阈值 = 平移；原地松开 = 插入锚点
                line.drag_start_snap = (snapped_tick, snapped_key);
                line.drag_start_raw = raw;
                line.drag_line_orig = line.anchors.clone();
                line.drag_confirmed = false;
                line.interaction = LineToolInteraction::DraggingLine { segment };
            }
            None => {
                // 空白处：清空并从此点开始新路径
                line.reset();
                line.push_anchor((snapped_tick, snapped_key));
            }
        }
    }

    /// 路径移动处理（增量式拖动）
    ///
    /// - 端点锚点/整条平移：以按下时吸附值为基准累加增量（保持网格对齐）；
    /// - 中间锚点/控制柄：以按下时原始值为基准（自由精确定位）。
    pub(super) fn handle_line_tool_moved(
        &mut self,
        snapped_tick: f32,
        snapped_key: f32,
        raw_tick: f32,
        raw_key: f32,
    ) {
        let max_key = self.editor_state.view.key_count.saturating_sub(1) as f32;
        let line = &mut self.editor_state.line_tool;
        let snap_delta = (
            snapped_tick - line.drag_start_snap.0,
            snapped_key - line.drag_start_snap.1,
        );
        let raw_delta = (
            raw_tick - line.drag_start_raw.0,
            raw_key - line.drag_start_raw.1,
        );
        match line.interaction {
            LineToolInteraction::DraggingAnchor(idx) => {
                // 端点吸附、中间锚点自由
                let is_endpoint = idx == 0 || idx + 1 >= line.anchors.len();
                let delta = if is_endpoint { snap_delta } else { raw_delta };
                let orig = line.drag_anchor_orig;
                if let Some(a) = line.anchors.get_mut(idx) {
                    a.pos = (
                        (orig.pos.0 + delta.0).max(0.0),
                        (orig.pos.1 + delta.1).clamp(0.0, max_key),
                    );
                }
                // 锚点移动后重算自动柄：未弯曲的段保持精确直线
                line.recompute_auto_handles();
            }
            LineToolInteraction::DraggingLine { .. } => {
                // 阈值确认：未确认前不应用平移（点击插入由 released 判定）
                if !line.drag_confirmed {
                    let v = &self.editor_state.view;
                    let dist_px = ((raw_delta.0 * v.zoom_x).powi(2)
                        + (raw_delta.1 * v.zoom_y).powi(2))
                    .sqrt();
                    if dist_px >= PRESS_DRAG_THRESHOLD_PX {
                        line.drag_confirmed = true;
                    } else {
                        return;
                    }
                }
                // 平移整条路径（吸附增量：端点保持落格）
                let origs = line.drag_line_orig.clone();
                for (i, a) in line.anchors.iter_mut().enumerate() {
                    let orig = origs.get(i).copied().unwrap_or(*a);
                    a.pos = (
                        (orig.pos.0 + snap_delta.0).max(0.0),
                        (orig.pos.1 + snap_delta.1).clamp(0.0, max_key),
                    );
                }
            }
            LineToolInteraction::DraggingHandle { anchor_idx, side } => {
                let orig = line.drag_handle_orig;
                let new_handle = (orig.0 + raw_delta.0, orig.1 + raw_delta.1);
                if let Some(a) = line.anchors.get_mut(anchor_idx) {
                    // 用户自定义柄：标记后不再被自动重算覆盖
                    match side {
                        HandleSide::In => a.set_in_handle(new_handle),
                        HandleSide::Out => a.set_out_handle(new_handle),
                    }
                }
            }
            LineToolInteraction::None => {}
        }
    }

    /// 路径释放处理
    ///
    /// 曲线段按下且未确认拖动（未超阈值）→ 视为点击 → 在该段中间插入锚点。
    pub(super) fn handle_line_tool_released(&mut self) {
        let line = &mut self.editor_state.line_tool;
        if let LineToolInteraction::DraggingLine { segment } = line.interaction {
            // 未确认拖动 → 点击插入锚点（位置 = 按下处，不吸附网格）
            if !line.drag_confirmed {
                line.insert_anchor_at(segment + 1, line.drag_start_raw);
            }
        }
        line.interaction = LineToolInteraction::None;
        line.drag_confirmed = false;
    }

    /// 双击处理：命中锚点（含与其重合的控制柄）→ 删除中间锚点（端点不可删）
    pub(super) fn handle_line_tool_double_clicked(&mut self, pos: Point) {
        match self.line_tool_hit_test(pos) {
            Some(LineToolHit::Anchor(idx))
            | Some(LineToolHit::Handle {
                anchor_idx: idx, ..
            }) => {
                self.editor_state.line_tool.delete_anchor(idx);
            }
            _ => {}
        }
    }

    /// y 坐标 → key（f32 原始值，不取整；中间锚点自由定位用）
    pub(super) fn raw_y_to_key(&self, y: f32) -> f32 {
        let v = &self.editor_state.view;
        let max_key_index = (v.visible_key_count - 1) as f32;
        max_key_index - (y - v.ruler_height + v.scroll_y) / v.zoom_y
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

    /// 路径命中测试（控制柄 > 锚点 > 曲线段；仅路径完整时有效）
    pub fn line_tool_hit_test(&self, pos: Point) -> Option<LineToolHit> {
        let line = &self.editor_state.line_tool;
        let anchors = &line.anchors;
        if anchors.len() < 2 {
            return None;
        }
        // 1. 控制柄（首锚点 out、尾锚点 in、中间 in+out）
        //    柄与锚点重合（未弯曲）时不参与命中——此时点击拖动锚点
        for (i, anchor) in anchors.iter().enumerate() {
            let ap = self.line_pos_screen_pos(anchor.pos);
            for side in line.visible_handle_sides(i) {
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
                        anchor_idx: i,
                        side,
                    });
                }
            }
        }
        // 2. 锚点
        for (i, anchor) in anchors.iter().enumerate() {
            let ap = self.line_pos_screen_pos(anchor.pos);
            if (pos.x - ap.x).hypot(pos.y - ap.y) <= ANCHOR_HIT_RADIUS {
                return Some(LineToolHit::Anchor(i));
            }
        }
        // 3. 曲线段（采样折线逼近）
        for (i, pair) in anchors.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let pa = self.line_pos_screen_pos(a.pos);
            let p1 = self.line_pos_screen_pos(a.out_handle_abs());
            let p2 = self.line_pos_screen_pos(b.in_handle_abs());
            let pb = self.line_pos_screen_pos(b.pos);
            if geom::point_curve_distance(pos, pa, p1, p2, pb) <= LINE_HIT_THRESHOLD {
                return Some(LineToolHit::Segment(i));
            }
        }
        None
    }

    /// 确认路径：按曲线经过的网格格点批量生成音符（√ 按钮）
    ///
    /// 生成规则：每段贝塞尔离散化后取格点，每个格点一个音符、
    /// 长度 = 当前吸附精度；写入当前音轨并使用 `CreateOp` 操作日志。
    /// 成功后清空路径状态；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        let snap = self.editor_state.view.snap_precision;
        let anchors = self.editor_state.line_tool.anchors.clone();
        if anchors.len() < 2 {
            return false;
        }
        // 逐段离散化收集格点（段间连接点相邻重复，整体去重）
        let mut points = Vec::new();
        for pair in anchors.windows(2) {
            points.extend(geom::curve_cell_points(pair[0], pair[1], snap));
        }
        points.dedup();
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
        // 清空路径状态并驱动渲染刷新
        self.editor_state.line_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消路径（× 按钮）
    pub(crate) fn cancel_line_tool(&mut self) {
        self.editor_state.line_tool.reset();
    }
}
