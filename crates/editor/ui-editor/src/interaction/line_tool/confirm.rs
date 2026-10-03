//! 曲线工具确认/取消：路径 + 填充区域 → 批量生成音符（√ / × 按钮）
//!
//! 生成规则（蜘蛛网 Spiderweb 式，**不使用"设定精度"**）：
//! 1. 每条路径按几何容差展平为折线（[`geom::flatten_path`]），再用
//!    [`paths::path_notes`] 逐音高行解析求出跨越边界；每个音高行一条音符，
//!    起点 = 进入该行的 tick、终点 = 下一条音符的起点 → 无缝、长度自然变化；
//! 2. 颜料桶标记的封闭区域用 [`fill::fill_spans`] 逐音高行求**内部区间**，
//!    区间端点 = 闭环边与行边界的解析交点，每个区间一条音符；
//! 3. 两部分合并后同 tick 同 key 只留最长（[`paths::keep_longest`]），
//!    写入当前音轨并使用 `CreateOp` 操作日志。
//!
//! 从 `line_tool.rs` 拆出（文件长度纪律）：交互处理与提交职责分离。

use super::{fill, geom, paths};
use crate::{Editor, Note};
use lumino_note_core::history::CreateOp;

/// 填充的可计算 tick 范围 = 画布可见区间 ∪ **图形自身的 tick 跨度**。
///
/// - 画布可见区间：背景填充（标记在环外）蔓延到画布边缘，与渲染背景矩形一致；
/// - 图形跨度：图形越出视图时内部填充不被可见范围裁掉
///   （旧实现按 snap 向上取整，会悄悄把填充截到网格边界）。
fn fill_tick_range(editor: &Editor, loops: &[Vec<(f32, f32)>]) -> (f32, f32) {
    let visible = if editor.editor_state.is_vertical_roll {
        let es = &editor.editor_state;
        let grid_h = (es.canvas.size_y - es.view.keyboard_width).max(0.0);
        let lo = (es.view.scroll_x / es.view.zoom_x).max(0.0);
        let hi = ((es.view.scroll_x + grid_h) / es.view.zoom_x).max(lo + 1.0);
        (lo, hi)
    } else {
        let lo = editor.x_to_tick(0.0).max(0.0);
        let hi = editor
            .x_to_tick(editor.editor_state.canvas.size_x)
            .max(lo + 1.0);
        (lo, hi)
    };
    let mut range = visible;
    for lp in loops {
        for &(tick, _) in lp {
            if tick.is_finite() {
                range.0 = range.0.min(tick);
                range.1 = range.1.max(tick);
            }
        }
    }
    range
}

impl Editor {
    /// 确认全部路径与填充：按蜘蛛网式逐音高行算法批量生成音符（√ 按钮）。
    ///
    /// 成功后清空全部路径、填充与编辑历史；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        // Conductor 音轨（track 0）禁止放置音符：整曲线工具不可用，
        // 与铅笔 finish_drawing、文字工具 confirm_text_tool 同源守卫
        if self.editor_state.data.current_track == 0 {
            tracing::debug!("曲线工具: Conductor 轨道禁止放置音符");
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        let snap_max = snap.max(1.0);
        let line_paths = self.editor_state.line_tool.paths.clone();
        let fill_marks = self.editor_state.line_tool.fill.clone();

        // ① 路径轮廓 → 音符：几何容差展平后逐音高行解析求交（无网格量化）
        let mut raw: Vec<paths::RawNote> = Vec::new();
        for path in &line_paths {
            if path.len() < 2 {
                continue;
            }
            let poly = geom::flatten_path(path);
            raw.extend(paths::path_notes(&poly, false));
        }

        // ② 颜料桶填充 → 逐音高行内部区间
        if !fill_marks.is_empty() {
            let key_count = self.editor_state.view.key_count;
            let edges = fill::collect_edges(&line_paths, snap_max);
            let loops = fill::assemble_loops(&edges);
            let regions = fill::mark_regions(&loops, &fill_marks, snap_max);
            let (tick_lo, tick_hi) = fill_tick_range(self, &loops);
            raw.extend(fill::fill_spans(
                &loops,
                &regions,
                tick_lo,
                tick_hi,
                0,
                key_count.saturating_sub(1) as i32,
            ));
        }

        // ③ 同 tick 同 key 只留最长（轮廓与填充大量重叠）+ 钳到合法 tick / key
        let key_count = self.editor_state.view.key_count as i32;
        let notes: Vec<(f32, u16, f32)> = paths::keep_longest(&raw)
            .into_iter()
            .filter(|n| n.key >= 0 && n.key < key_count)
            .map(|n| {
                // 起点钳到 0；时长至少 1 tick（`RawNote::length` 已保证）
                (n.start.max(0) as f32, n.key as u16, n.length() as f32)
            })
            .collect();
        if notes.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let mut create_ops = Vec::with_capacity(notes.len());
        for (start, key, length) in notes {
            let note = Note::new(start, key, length);
            // 按值记录：redo 按值重插，undo 按值删除（删加语义，无 ID）
            if self
                .editor_state
                .data
                .insert_note_with_id(track, note.clone())
                .is_some()
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
