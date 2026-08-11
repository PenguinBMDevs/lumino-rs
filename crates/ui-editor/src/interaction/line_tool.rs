//! 曲线工具贝塞尔路径交互：多条曲线批量绘制 → √ 批量生成音符
//!
//! 交互流程：
//! - 前两次左键按下：设置首尾端点（tick 吸附、key 整数格），创建一条路径；
//! - 空白处按下：**开始新路径**（保留已有路径，支持同时绘制多条曲线）；
//! - 路径完整后：
//!   - 点击曲线段（原地松开）：在该段中间插入锚点（**不吸附网格**）；
//!   - 按下曲线段 + 拖动（超过 4px）：整条路径平移；
//!   - 拖动锚点：端点吸附网格、中间锚点自由移动；
//!   - 拖动控制柄：自由弯曲对应贝塞尔段；
//!   - 双击中间锚点：删除；
//!   - 放置新锚点（设置端点/插入）时吸附到周围锚点（仅认锚点不认控制柄）；
//! - 所有路径共享一组 √（批量确认生成音符）/ ×（批量取消）按钮；
//! - **路径编辑历史**：创建路径（合并为一次）、拖动锚点/控制柄/平移、
//!   插入/删除锚点为一次撤销操作（Ctrl+Z / Ctrl+Y）。
//!
//! 交互状态独立于 `EditState`（`LineToolInteraction`），不耦合音符选择机制。
//! 纯几何算法（贝塞尔求值/距离/格点离散化）在 `line_tool/geom.rs`，
//! 命中测试/坐标转换/放置吸附在 `line_tool/hit_test.rs`。

mod fill;
mod geom;
mod hit_test;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_confirm;
#[cfg(test)]
mod tests_fill;

use crate::{Editor, Note};
use iced_core::Point;
use lumino_editor_state::{BezierAnchor, HandleSide, LineToolInteraction};
use lumino_note_core::history::CreateOp;

/// 曲线段按下 → 拖动判定阈值（像素）：未超阈值松开视为点击插入锚点
const PRESS_DRAG_THRESHOLD_PX: f32 = 4.0;

/// 路径命中类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineToolHit {
    /// 命中指定路径的锚点
    Anchor { path: usize, idx: usize },
    /// 命中控制柄
    Handle {
        path: usize,
        anchor_idx: usize,
        side: HandleSide,
    },
    /// 命中曲线段（索引 = anchors[i] → anchors[i+1]）
    Segment { path: usize, segment: usize },
}

impl Editor {
    /// 路径按下处理
    ///
    /// - 存在创建中的路径（最后一条 < 2 锚点）：续完端点（吸附到周围锚点）；
    ///   否则新建路径（保留已有路径）。
    /// - 完整路径：按 控制柄 > 锚点 > 曲线段 > 空白 的优先级分发。
    pub(super) fn handle_line_tool_pressed(
        &mut self,
        pos: Point,
        snapped_tick: f32,
        snapped_key: f32,
    ) {
        let raw = (self.x_to_tick(pos.x), self.raw_y_to_key(pos.y));
        // 端点设置：创建/续完路径（连续追加合并为一次撤销）
        if !self.editor_state.line_tool.is_complete()
            || self.editor_state.line_tool.creating_path().is_some()
        {
            let anchor_pos = self
                .snap_new_anchor(raw, &[])
                .unwrap_or((snapped_tick, snapped_key));
            let line = &mut self.editor_state.line_tool;
            match line.creating_path() {
                None => {
                    // 新建路径：记录新状态（undo 一次删除整条）
                    line.paths.push(Vec::new());
                    line.push_anchor(line.paths.len() - 1, anchor_pos);
                    line.push_path_history();
                    line.last_push_path = Some(line.paths.len() - 1);
                }
                Some(pi) => {
                    // 续完创建中的路径：连续追加合并到同一历史
                    line.push_anchor(pi, anchor_pos);
                    if line.last_push_path == Some(pi) {
                        line.update_top_path_history();
                    } else {
                        line.push_path_history();
                    }
                    line.last_push_path = Some(pi);
                }
            }
            return;
        }
        // 完整路径：命中分发（先 &self 命中测试，再取可变引用）
        let hit = self.line_tool_hit_test(pos);
        // 空白处按下需要吸附位置（预先计算，避免借用冲突）
        let blank_anchor = self.snap_new_anchor(raw, &[]);
        let line = &mut self.editor_state.line_tool;
        // 任何命中拖动/空白新建都是新操作（打断创建合并）
        line.last_push_path = None;
        match hit {
            Some(LineToolHit::Handle {
                path,
                anchor_idx,
                side,
            }) => {
                let anchor = &line.paths[path][anchor_idx];
                line.drag_handle_orig = match side {
                    HandleSide::In => anchor.in_handle,
                    HandleSide::Out => anchor.out_handle,
                };
                line.drag_start_raw = raw;
                line.drag_confirmed = true;
                line.interaction = LineToolInteraction::DraggingHandle {
                    path,
                    anchor_idx,
                    side,
                };
            }
            Some(LineToolHit::Anchor { path, idx }) => {
                line.drag_start_snap = (snapped_tick, snapped_key);
                line.drag_start_raw = raw;
                line.drag_anchor_orig = line.paths[path][idx];
                line.drag_confirmed = true;
                line.interaction = LineToolInteraction::DraggingAnchor { path, idx };
            }
            Some(LineToolHit::Segment { path, segment }) => {
                // 按下待定：移动超阈值 = 平移；原地松开 = 插入锚点
                line.drag_start_snap = (snapped_tick, snapped_key);
                line.drag_start_raw = raw;
                line.drag_line_orig = line.paths[path].clone();
                line.drag_confirmed = false;
                line.interaction = LineToolInteraction::DraggingLine { path, segment };
            }
            None => {
                // 空白处：开始新路径（保留已有路径——批量绘制）
                line.paths.push(vec![BezierAnchor::new(
                    blank_anchor.unwrap_or((snapped_tick, snapped_key)),
                )]);
                line.push_path_history();
                line.last_push_path = Some(line.paths.len() - 1);
            }
        }
    }

    /// 路径移动处理（增量式拖动）
    ///
    /// - 端点锚点/整条平移：以按下时吸附值为基准累加增量（保持网格对齐）；
    /// - 中间锚点/控制柄：以按下时原始值为基准（自由精确定位）；
    /// - 拖动锚点磁吸：目标位置 16px（屏幕距离）内存在**其他路径**的锚点
    ///   时，自动吸附到该锚点（支持闭合图形/端点对齐）；同路径锚点不参与
    ///   磁吸（避免拖动时被相邻锚点卡住造成路径退化）。
    pub(super) fn handle_line_tool_moved(
        &mut self,
        snapped_tick: f32,
        snapped_key: f32,
        raw_tick: f32,
        raw_key: f32,
    ) {
        let max_key = self.editor_state.view.key_count.saturating_sub(1) as f32;
        let (start_snap, start_raw) = {
            let line = &self.editor_state.line_tool;
            (line.drag_start_snap, line.drag_start_raw)
        };
        let snap_delta = (snapped_tick - start_snap.0, snapped_key - start_snap.1);
        let raw_delta = (raw_tick - start_raw.0, raw_key - start_raw.1);
        match self.editor_state.line_tool.interaction {
            LineToolInteraction::DraggingAnchor { path, idx } => {
                // 目标位置：端点吸附网格、中间锚点自由
                let is_endpoint = {
                    let cur_path = &self.editor_state.line_tool.paths[path];
                    idx == 0 || idx + 1 >= cur_path.len()
                };
                let delta = if is_endpoint { snap_delta } else { raw_delta };
                let orig = self.editor_state.line_tool.drag_anchor_orig;
                let target = (
                    (orig.pos.0 + delta.0).max(0.0),
                    (orig.pos.1 + delta.1).clamp(0.0, max_key),
                );
                // 磁吸：先 &self 计算（排除同路径锚点），再 &mut 应用
                let magnet = self.snap_anchor_drag(target, path);
                let line = &mut self.editor_state.line_tool;
                if let Some(cur_path) = line.paths.get_mut(path)
                    && let Some(a) = cur_path.get_mut(idx)
                {
                    a.pos = magnet.unwrap_or(target);
                }
                // 锚点移动后重算自动柄：未弯曲的段保持精确直线
                line.recompute_auto_handles();
            }
            LineToolInteraction::DraggingLine { path, .. } => {
                // 阈值确认：未确认前不应用平移（点击插入由 released 判定）
                let (confirmed, drag_line_orig) = {
                    let line = &self.editor_state.line_tool;
                    (line.drag_confirmed, line.drag_line_orig.clone())
                };
                if !confirmed {
                    let v = &self.editor_state.view;
                    let dist_px = ((raw_delta.0 * v.zoom_x).powi(2)
                        + (raw_delta.1 * v.zoom_y).powi(2))
                    .sqrt();
                    if dist_px >= PRESS_DRAG_THRESHOLD_PX {
                        let line = &mut self.editor_state.line_tool;
                        line.drag_confirmed = true;
                    } else {
                        return;
                    }
                }
                // 平移整条路径（吸附增量：端点保持落格）
                let line = &mut self.editor_state.line_tool;
                let Some(cur_path) = line.paths.get_mut(path) else {
                    return;
                };
                for (i, a) in cur_path.iter_mut().enumerate() {
                    let orig = drag_line_orig.get(i).copied().unwrap_or(*a);
                    a.pos = (
                        (orig.pos.0 + snap_delta.0).max(0.0),
                        (orig.pos.1 + snap_delta.1).clamp(0.0, max_key),
                    );
                }
            }
            LineToolInteraction::DraggingHandle {
                path,
                anchor_idx,
                side,
            } => {
                let orig = {
                    let line = &self.editor_state.line_tool;
                    line.drag_handle_orig
                };
                let new_handle = (orig.0 + raw_delta.0, orig.1 + raw_delta.1);
                let line = &mut self.editor_state.line_tool;
                if let Some(a) = line.paths.get_mut(path).and_then(|p| p.get_mut(anchor_idx)) {
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
    /// 曲线段按下且未确认拖动（未超阈值）→ 视为点击 → 在该段中间插入锚点，
    /// 吸附到周围锚点（被点击段自身两端除外，避免产生零长段）。
    /// 状态产生变化（插入/拖动）→ 记录一次历史；未变化不记录。
    pub(super) fn handle_line_tool_released(&mut self) {
        // 先读取待处理数据（避免与后续可变借用冲突）
        let pending = match self.editor_state.line_tool.interaction {
            LineToolInteraction::DraggingLine { path, segment } => {
                Some((path, segment, self.editor_state.line_tool.drag_start_raw))
            }
            _ => None,
        };
        // 未确认拖动 → 点击插入锚点（位置 = 按下处，不吸附网格；仅锚点吸附）
        if let Some((path, segment, raw)) = pending
            && !self.editor_state.line_tool.drag_confirmed
        {
            let pos = self
                .snap_new_anchor(raw, &[(path, segment), (path, segment + 1)])
                .unwrap_or(raw);
            self.editor_state
                .line_tool
                .insert_anchor_at(path, segment + 1, pos);
        }
        let line = &mut self.editor_state.line_tool;
        line.interaction = LineToolInteraction::None;
        line.drag_confirmed = false;
        // 状态产生变化（插入锚点/拖动平移）→ 记录历史；未变化（空点击）不记录
        if line.snapshot() != line.path_history[line.path_history_index] {
            line.push_path_history();
        }
        line.last_push_path = None;
    }

    /// 双击处理：命中锚点（含与其重合的控制柄）→ 删除中间锚点（端点不可删），
    /// 删除后记录一次历史
    pub(super) fn handle_line_tool_double_clicked(&mut self, pos: Point) {
        match self.line_tool_hit_test(pos) {
            // 命中锚点（含与其重合的控制柄）且删除成功（中间锚点）→ 记录历史
            Some(LineToolHit::Anchor { path, idx })
            | Some(LineToolHit::Handle {
                path,
                anchor_idx: idx,
                ..
            }) if self.editor_state.line_tool.delete_anchor(path, idx) => {
                let line = &mut self.editor_state.line_tool;
                line.push_path_history();
                line.last_push_path = None;
            }
            _ => {}
        }
    }

    /// 确认全部路径与填充：按路径曲线经过的格点 + 颜料桶已填充格点
    /// 批量生成音符（√ 按钮）
    ///
    /// 生成规则：每条完整路径逐段贝塞尔离散化取格点，合并颜料桶填充的
    /// 封闭区域内部格点，去重后每格一个音符、长度 = 当前吸附精度；
    /// 写入当前音轨并使用 `CreateOp` 操作日志。
    /// 成功后清空全部路径、填充与编辑历史；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        let snap = self.editor_state.view.snap_precision;
        let line = &self.editor_state.line_tool;
        let paths = line.paths.clone();
        let fill_cells = line.fill.clone();
        // 逐路径逐段离散化收集格点（段间连接点相邻重复，整体去重）
        let mut points = Vec::new();
        for path in &paths {
            if path.len() < 2 {
                continue;
            }
            for pair in path.windows(2) {
                points.extend(geom::curve_cell_points(pair[0], pair[1], snap));
            }
        }
        // 合并颜料桶已填充格点（封闭区域内部 → 实心），整体去重
        // （tick 转格索引去重，f32 不实现 Hash 不能直接做 key）
        points.extend(fill_cells);
        let snap_key = snap.max(1.0);
        let mut seen = std::collections::HashSet::new();
        points.retain(|p| seen.insert(((p.0 / snap_key).round() as i64, p.1)));
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
        // 清空全部路径与历史并驱动渲染刷新
        self.editor_state.line_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消全部路径（× 按钮）
    pub(crate) fn cancel_line_tool(&mut self) {
        self.editor_state.line_tool.reset();
    }
}
