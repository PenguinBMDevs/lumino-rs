//! 曲线工具确认/取消：路径格点 + 填充区域 → 批量生成音符（√ / × 按钮）
//!
//! 从 `line_tool.rs` 拆出（文件长度纪律）：交互处理与提交职责分离。

use super::{fill, geom};
use crate::{Editor, Note};
use lumino_note_core::history::CreateOp;

impl Editor {
    /// 确认全部路径与填充：按路径曲线经过的格点 + 颜料桶已填充格点
    /// 批量生成音符（√ 按钮）
    ///
    /// 生成规则：每条完整路径逐段贝塞尔离散化取格点，合并颜料桶填充的
    /// 封闭区域内部格点，去重后每格一个音符、长度 = 当前吸附精度；
    /// 写入当前音轨并使用 `CreateOp` 操作日志。
    /// 成功后清空全部路径、填充与编辑历史；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        let snap = self.editor_state.view.snap_precision;
        let snap_max = snap.max(1.0);
        let line = &self.editor_state.line_tool;
        let paths = line.paths.clone();
        let fill_marks = line.fill.clone();
        // 逐路径逐段离散化收集格点（段间连接点相邻重复，整体去重）。
        // 每条路径**最后一段的终点格点** = 路径终点锚点：其音符尾部对齐
        // 锚点（tick - snap 后与相邻格点去重合并 → 最后一个音符
        // [tick-snap, tick)，而非原行为的 [tick, tick+snap) 头部对齐）。
        // tick < snap 时保持原样（避免负 tick）。
        let mut points = Vec::new();
        for path in &paths {
            if path.len() < 2 {
                continue;
            }
            let last_seg = path.len() - 2;
            for (si, pair) in path.windows(2).enumerate() {
                let mut seg_points = geom::curve_cell_points(pair[0], pair[1], snap);
                if si == last_seg
                    && let Some((tick, _)) = seg_points.last_mut()
                    && *tick >= snap
                {
                    *tick -= snap;
                }
                points.extend(seg_points);
            }
        }
        // 填充音符：√ 确认时按**图形覆盖范围**计算（标记 → 区域 → 全部格点）。
        // 范围 = 画布可见 tick 区间 + 全键盘 key（与渲染背景矩形一致，纵向转置）。
        if !fill_marks.is_empty() {
            let (tick_lo, tick_hi) = if self.editor_state.is_vertical_roll {
                let view = &self.editor_state.view;
                let canvas_h = self.editor_state.canvas.size_y;
                let kb_h = view.keyboard_width;
                let grid_h = (canvas_h - kb_h).max(0.0);
                let lo = (view.scroll_x / view.zoom_x).max(0.0);
                let hi = ((view.scroll_x + grid_h) / view.zoom_x).max(lo + snap_max);
                (lo, hi)
            } else {
                let lo = self.x_to_tick(0.0).max(0.0);
                let hi = self
                    .x_to_tick(self.editor_state.canvas.size_x)
                    .max(lo + snap_max);
                (lo, hi)
            };
            let key_count = self.editor_state.view.key_count;
            let edges = fill::collect_edges(&paths, snap_max);
            let cells = fill::confirm_fill_cells(
                &edges,
                snap_max,
                &fill_marks,
                (
                    (tick_lo / snap_max).floor() as i64,
                    (tick_hi / snap_max).ceil() as i64,
                ),
                (0, key_count.saturating_sub(1)),
            );
            points.extend(cells.iter().map(|&(ti, k)| (ti as f32 * snap_max, k)));
        }
        // 整体去重（tick 转格索引去重，f32 不实现 Hash 不能直接做 key）
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
