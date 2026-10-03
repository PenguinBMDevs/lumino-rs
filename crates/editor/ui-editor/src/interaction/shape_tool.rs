//! 形状工具拖拽绘制：矩形/圆/三角 拉出 → √ 批量确认生成音符
//!
//! 与 `line_tool` 同为「拖拽 → 实时预览 → √ 固化」，但纯拉框范式：形状在 √ 确认前
//! 为临时叠加，确认后「固化」为音符（**每格一个、长度 = 吸附精度**），不保存为矢量对象。
//! 填充桶（`fill_enabled`）决定确认时是否额外生成图形内部音符。
//!
//! ⚠️ 音符生成**尚未**对齐曲线工具的蜘蛛网（Spiderweb）式处理：曲线工具
//! （`line_tool/paths.rs` + `line_tool/fill/spans.rs`）已改为按几何交点生成无缝
//! 变长音符、不再依赖设定精度；形状工具仍是「按 snap 网格枚举格点、每格一个定长音符」。
//!
//! **切分档位（x 分音符）**：填充桶的 `fill_division` 是共享模式设置，形状工具
//! 已接入——**填充图形**（`filled = true`）的格点先按行合并成连续区间，再用与曲线
//! 工具同源的 [`fill::spans::chop_span`] 按全局网格切成 x 分音符；**轮廓图形**
//! 不在填充桶职责内，保持原「每格一条 snap 长音符」不变（与曲线工具里
//! 「切分只作用于填充区间、轮廓走 `path_notes`」完全同构）。
//! 未开切分档位时行为与改动前完全一致。

use std::collections::{BTreeMap, HashSet};

use lumino_editor_state::shape_tool::point_in_shape;
use lumino_editor_state::ShapeNote;
use lumino_note_core::history::CreateOp;

use crate::interaction::line_tool::fill::spans::{chop_span, division_step};
use crate::{Editor, Note};

/// 按音高行把格点合并成连续区间，再按 x 分音符的全局网格切分。
///
/// 输入格点来自 `shape_cells`：tick = snap 整数倍、key 为整数行。同行的格点
/// 间隔恰为 `snap` 时视为连续（同一段覆盖），合并成区间 `[首, 末 + snap)`
/// （末格自身占一份长度，与曲线的"区间右端"语义一致）。
///
/// 切分复用曲线工具填充的 [`chop_span`]：内部切点落在全局网格线上、首尾残段
/// 保留 → 覆盖严格等于原格点覆盖，且与曲线工具的切分结果同口径。
fn chop_cells(cells: &[(f32, u16)], snap: f32, step: f32) -> Vec<(f32, u16, f32)> {
    let snap = snap.max(1.0);
    // 按音高行分组（BTreeMap 保证输出按 key 有序，便于测试断言）
    let mut by_key: BTreeMap<u16, Vec<f32>> = BTreeMap::new();
    for &(tick, key) in cells {
        by_key.entry(key).or_default().push(tick);
    }

    let mut out = Vec::new();
    for (key, mut ticks) in by_key {
        ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ticks.dedup();

        let mut start: Option<f32> = None;
        let mut prev: Option<f32> = None;
        let flush = |start: Option<f32>, prev: Option<f32>, out: &mut Vec<_>| {
            if let (Some(s), Some(p)) = (start, prev) {
                for (a, b) in chop_span(s, p + snap, step) {
                    out.push((a, key, b - a));
                }
            }
        };
        for t in ticks {
            let contiguous = prev.is_some_and(|p| t - p <= snap * 1.001);
            if !contiguous {
                flush(start, prev, &mut out);
                start = Some(t);
            }
            prev = Some(t);
        }
        flush(start, prev, &mut out);
    }
    out
}

impl Editor {
    /// 形状工具：左键按下 —— 开始拖拽拉框
    ///
    /// - Conductor 轨道（track 0）：整工具不可用，直接返回；
    /// - 填充桶开启且点击命中某待确认图形内部：标记该图形为「已填充」
    ///   （支持用填充桶填充已拉出的图案），不开始新拖拽；
    /// - 否则开始拖拽拉框。
    ///
    /// `tick` / `key` 已是**调用方按 Shift 状态解析好的坐标**：
    /// Shift 按住时为鼠标原始浮点坐标（绕过 key/音符精度吸附，自由跟随鼠标），
    /// 否则为网格吸附后的坐标。正图形约束在 `effective_rect` 内基于该坐标计算。
    pub(crate) fn handle_shape_tool_pressed(&mut self, tick: f32, key: f32, _shift: bool) {
        // Conductor 音轨：形状工具不可用
        if self.editor_state.data.current_track == 0 {
            return;
        }
        // Ctrl+单击 = 打开「分音符填充」对话框（与曲线工具下的填充桶同行为），
        // 不标记填充、不开始拖拽。弹窗为主窗口覆盖层：只置请求位，由 Root 取走。
        if self.ctrl_pressed() {
            self.fill_division_dialog_requested = true;
            tracing::info!("形状工具: Ctrl+单击 → 请求打开分音符填充对话框");
            return;
        }
        // 填充桶：点击待确认图形内部 → 标记填充
        if self.editor_state.shape_tool.fill_enabled
            && let Some(idx) = self.shape_hit_test(tick, key)
        {
            self.editor_state.shape_tool.shapes[idx].filled = true;
            self.mark_notes_changed();
            return;
        }
        // 正常：开始拖拽拉框（坐标已由调用方解析：Shift=原始浮点 / 否则=网格吸附）
        self.editor_state.shape_tool.begin_drag((tick, key));
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
        if self.editor_state.shape_tool.end_drag(snap, shift).is_some() {
            self.mark_notes_changed();
        }
    }

    /// 形状工具：确认（√）—— 把所有待确认图形转成音符
    ///
    /// - **轮廓图形**：每格一条、长度 = snap（既有行为，不受切分档位影响）；
    /// - **填充图形**：未开切分档位时同上；开启时按行合并连续格点后用
    ///   x 分音符的全局网格切分（与曲线工具填充同源的 [`chop_span`]）。
    ///
    /// 流程与 `confirm_line_tool` 同构：先整体去重，再批量插入并写入历史。
    pub(crate) fn confirm_shape_tool(&mut self) -> bool {
        // Conductor 音轨：形状不可用
        if self.editor_state.data.current_track == 0 {
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        let snap_key = snap.max(1.0);
        // 填充桶切分档位（共享的模式设置，权威在 line_tool，与 fill_enabled 同构）
        let step = self
            .editor_state
            .line_tool
            .fill_division
            .map(|x| division_step(x, self.editor_state.view.ppq));

        // 屏幕空间约束所需的像素尺度（tick/key 每单位像素数）
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;

        // 定长（snap）音符：轮廓图形 + 未开切分的填充图形
        // （第三项 = 来源图形索引，供逐图形登记音符归属）
        let mut points: Vec<(f32, u16, usize)> = Vec::new();
        // 切分音符：(起始 tick, key, 长度, 来源图形索引)
        let mut chopped: Vec<(f32, u16, f32, usize)> = Vec::new();
        for (idx, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            let cells = lumino_editor_state::shape_tool::shape_cells(
                shape.kind,
                shape.rect,
                shape.shift_constrained,
                shape.filled,
                snap,
                px_per_tick,
                px_per_key,
            );
            match (shape.filled, step) {
                (true, Some(s)) => chopped.extend(
                    chop_cells(&cells, snap, s)
                        .into_iter()
                        .map(|(t, k, l)| (t, k, l, idx)),
                ),
                _ => points.extend(cells.into_iter().map(|(t, k)| (t, k, idx))),
            }
        }
        // 整体去重（跨多个图形 + 图形内部/边界可能重合）——先到先得，保留下标最小的图形
        let mut seen: HashSet<(i64, u16)> = HashSet::new();
        points.retain(|p| seen.insert(((p.0 / snap_key).round() as i64, p.1)));
        let mut seen_chopped: HashSet<(i64, u16)> = HashSet::new();
        chopped.retain(|p| seen_chopped.insert(((p.0 / snap_key).round() as i64, p.1)));

        if points.is_empty() && chopped.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        // 逐图形音符归属（与写入 payload 同源，仅多带来源信息）
        let mut per_shape_notes: Vec<Vec<ShapeNote>> =
            vec![Vec::new(); self.editor_state.shape_tool.shapes.len()];
        for &(tick, key, idx) in &points {
            if let Some(bucket) = per_shape_notes.get_mut(idx) {
                bucket.push(ShapeNote {
                    track,
                    tick,
                    key,
                    length: snap,
                });
            }
        }
        for &(tick, key, length, idx) in &chopped {
            if let Some(bucket) = per_shape_notes.get_mut(idx) {
                bucket.push(ShapeNote {
                    track,
                    tick,
                    key,
                    length,
                });
            }
        }
        let payload: Vec<Note> = points
            .iter()
            .map(|&(tick, key, _)| Note::new(tick, key, snap))
            .chain(
                chopped
                    .iter()
                    .map(|&(tick, key, length, _)| Note::new(tick, key, length)),
            )
            .collect();
        let mut create_ops: Vec<CreateOp> = Vec::with_capacity(payload.len());
        if payload.len() <= super::BATCH_INSERT_THRESHOLD {
            // 小规模：逐音符插入（当前轨自动记录 GPU 段内增量事件）
            for note in payload {
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
        } else {
            // 大规模：批量归并写入（单次 O(N+M)）。
            //
            // 逐音符插入会为每个音符记一条 `InsertAt`，渲染侧逐条发
            // `NoteEvent::Insert` 并在 GPU 内搬移其后全部实例（Σtail 超线性）——
            // 大面积形状 + 切分同样会一次生成上万音符，与曲线填充同源悬崖。
            let before = self.editor_state.data.current_track_note_count();
            self.editor_state.data.batch_insert_notes_with_ids(&payload);
            if self.editor_state.data.current_track_note_count() == before {
                // 音轨不存在等异常：未写入任何音符
                return false;
            }
            create_ops.extend(payload.iter().map(|note| CreateOp {
                track_id: track as u32,
                note: lumino_editor_state::note_to_event(note.clone()),
            }));
        }
        if create_ops.is_empty() {
            return false;
        }

        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        // 登记图形对象供「鼠标工具」点选/移动/删除（须在清空待确认列表之前读几何）
        self.record_shape_tool_shapes(per_shape_notes);
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
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        for (i, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            if point_in_shape(
                shape.kind,
                shape.rect,
                shape.shift_constrained,
                px_per_tick,
                px_per_key,
                tick,
                key,
            ) {
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
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
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
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
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
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
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
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
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
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(!editor.editor_state.shape_tool.shapes[0].filled);
        // 开启填充桶并点选图形内部（命中）→ 标记填充
        editor.editor_state.shape_tool.fill_enabled = true;
        editor.handle_shape_tool_pressed(2.0, 62.0, false);
        assert!(editor.editor_state.shape_tool.shapes[0].filled);
    }

    /// Shift 拉出：尺寸直接吃鼠标原始坐标（绕过 key/音符精度吸附），再套正图形约束。
    ///
    /// 用非整数原始坐标 (10.3, 63.7) 拖拽（宽高均不整除网格）。若错误地先吸附再约束
    /// （(10, 64)），生成的音符数与基于原始坐标不同——据此证明确实走了原始坐标。
    #[test]
    fn test_shift_uses_raw_mouse_coords_for_square() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        // 模拟 host 通道：Shift 按下（released 路径读取 shift_pressed 决定约束）
        editor.shift_pressed = true;
        // 起点 (0.0, 60.0)，Shift 拖到非网格坐标 (10.3, 63.7)（原始浮点）
        editor.handle_shape_tool_pressed(0.0, 60.0, true);
        editor.handle_shape_tool_moved(10.3, 63.7);
        editor.handle_shape_tool_released();
        // 存储的外接框应为原始鼠标坐标（未吸附到网格）：若被吸附会变成 (0,60,10,64)
        assert_eq!(
            editor.editor_state.shape_tool.shapes[0].rect,
            (0.0, 60.0, 10.3, 63.7),
            "Shift 拖拽应直接吃鼠标原始坐标，而非先吸附到网格"
        );
        let ok = editor.confirm_shape_tool();
        assert!(ok);

        // 与 confirm 一致的屏幕像素尺度（tick/key 每单位像素数）
        let px_per_tick = editor.editor_state.view.zoom_x;
        let px_per_key = editor.editor_state.view.zoom_y;

        // 预期：基于原始鼠标坐标 (10.3, 63.7) 在「屏幕空间」套 Shift 约束
        // （正方形：min(宽_px, 高_px)），量化到网格后的音符数，
        // 应等于直接对该原始外接框套约束的几何结果。
        let raw_expected = lumino_editor_state::shape_tool::shape_cells(
            ShapeKind::Rectangle,
            (0.0, 60.0, 10.3, 63.7),
            true,
            false,
            1.0,
            px_per_tick,
            px_per_key,
        )
        .len();
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            raw_expected,
            "确认生成的音符数应等于基于原始鼠标外接框套屏幕空间 Shift 约束的结果"
        );
    }

    // ── 切分档位（x 分音符填充）同步 ──

    #[test]
    fn test_filled_shape_uses_division_step() {
        // 填充矩形 0..4 × key 60..64，snap = 1、ppq = 480：
        // 四分音符档（480 tick 太长 → 每行只有 1 条）；改用 1/8 档看切分效果。
        let mut editor = test_editor();
        editor.editor_state.view.ppq = 480;
        editor.set_shape(ShapeKind::Rectangle);
        editor.set_fill_division(Some(8)); // 八分音符 = 240 tick
        editor.editor_state.shape_tool.fill_enabled = true;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(480.0, 60.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());

        let notes = editor.editor_state.data.current_track_notes();
        // 单行（key 60）：格点 0,1,2,...,480（snap=1）合并成 [0, 481) → 按 240 切
        let row: Vec<(u32, u32)> = notes
            .iter()
            .filter(|n| n.key == 60)
            .map(|n| (n.start_tick, n.end_tick))
            .collect();
        assert_eq!(
            row,
            vec![(0, 240), (240, 480), (480, 481)],
            "八分音符切分 + 尾部残段: {row:?}"
        );
    }

    #[test]
    fn test_outline_shape_ignores_division() {
        // 轮廓图形不在填充桶职责内：切分档位开启也保持「每格一条 snap 长音符」
        let mut editor = test_editor();
        editor.editor_state.view.ppq = 480;
        editor.set_shape(ShapeKind::Rectangle);
        editor.set_fill_division(Some(8));
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());
        // 与未开切分时完全一致（既有回归基线：16 条轮廓格音符，长度 1）
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            16,
            "轮廓图形不受切分档位影响"
        );
        assert!(
            editor
                .editor_state
                .data
                .current_track_notes()
                .iter()
                .all(|n| n.end_tick - n.start_tick == 1),
            "轮廓音符长度仍为 snap"
        );
    }

    #[test]
    fn test_no_division_keeps_legacy_behaviour() {
        // 未开切分 → 填充矩形仍是每格一条 snap 长音符（改动前的基线）
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = true;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());
        assert_eq!(editor.editor_state.data.current_track_note_count(), 25);
    }

    #[test]
    fn test_chop_cells_merges_contiguous_and_keeps_gaps() {
        // 同 key：0,1,2 连续（snap=1）→ 一段 [0,3)；5 孤立 → 另一段 [5,6)
        let cells = vec![(0.0f32, 60u16), (1.0, 60), (2.0, 60), (5.0, 60)];
        let out = chop_cells(&cells, 1.0, 2.0);
        assert_eq!(
            out,
            vec![(0.0, 60, 2.0), (2.0, 60, 1.0), (5.0, 60, 1.0)],
            "合并连续格点后按步长切分，空隙独立成段"
        );
    }

    #[test]
    fn test_shape_ctrl_click_requests_dialog() {
        // 形状工具下的 Ctrl+单击同样请求弹窗（与曲线工具填充桶一致）
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.set_ctrl_pressed(true);
        editor.handle_shape_tool_pressed(2.0, 62.0, false);
        assert!(
            editor.take_fill_division_dialog_request(),
            "形状工具 Ctrl+单击应置位弹窗请求"
        );
        assert!(
            !editor.editor_state.shape_tool.has_pending(),
            "Ctrl+单击不应开始拖拽"
        );
    }
}
