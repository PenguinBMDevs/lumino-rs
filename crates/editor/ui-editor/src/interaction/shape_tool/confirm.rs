//! 形状工具确认/取消：待确认图形 → 批量生成音符（√ / × 按钮）
//!
//! **描边（轮廓，`filled = false`）与曲线工具的轮廓同源**：形状边界先导出为闭合折线
//! （[`shape_outline_path`]），再交给曲线工具那一套蜘蛛网（Spiderweb）式逐音高行解析
//! （[`paths::path_notes`]）——每个音高行一条音符、起点 = 进入该行的 tick、终点 =
//! 下一条音符的起点 → **无缝连奏、长度自然变化**，全程不使用吸附精度；闭合环从最左点
//! 重启、竖直段各占 1 tick，与 `line_tool/paths.rs` 的轮廓口径完全一致。
//!
//! **填充（`filled = true`）不走这条路**：仍是「按 snap 网格枚举格点、每格一条定长
//! 音符」，开启「x 分音符」切分档位（共享的模式设置，权威在 `line_tool`）时先按行合并
//! 连续格点，再用与曲线填充同源的 [`chop_span`] 按全局网格切分——与曲线工具里「切分
//! 只作用于填充区间、轮廓走 `path_notes`」完全同构。切分档位对描边不生效。
//!
//! 写入按规模分流（见 `interaction::batch_insert`）：小规模逐音符插入（保留 GPU 段内
//! 增量事件），超过 `BATCH_INSERT_THRESHOLD` 走批量归并——逐音符写入会为每个音符产生
//! 一条 `InsertAt` 渲染事件，万级音符时在 GPU 内造成超线性实例搬移（见
//! `ui_fill_confirm_bench`）。
//!
//! 从 `shape_tool.rs` 拆出（文件长度纪律 REF-001：单文件 ≤ 400 行，含测试不豁免）。

use std::collections::{BTreeMap, HashSet};

use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_tool::{ShapeSpec, shape_cells, shape_outline_path};
use lumino_note_core::history::CreateOp;

use crate::interaction::line_tool::fill::spans::{chop_span, division_step};
use crate::interaction::line_tool::paths;
use crate::{Editor, Note};

/// 按音高行把格点合并成连续区间，再按 x 分音符的全局网格切分。
///
/// 输入格点来自 `shape_cells`：tick = snap 整数倍、key 为整数行。同行的格点
/// 间隔恰为 `snap` 时视为连续（同一段覆盖），合并成区间 `[首, 末 + snap)`
/// （末格自身占一份长度，与曲线的"区间右端"语义一致）。
///
/// 切分复用曲线工具填充的 [`chop_span`]：内部切点落在全局网格线上、首尾残段
/// 保留 → 覆盖严格等于原格点覆盖，且与曲线工具的切分结果同口径。
///
/// `pub(super)`：仅形状工具（含其测试）使用——填充腿的实现细节。
pub(super) fn chop_cells(cells: &[(f32, u16)], snap: f32, step: f32) -> Vec<(f32, u16, f32)> {
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

/// 形状**描边**（轮廓）→ 音符：与曲线工具轮廓**同源**的蜘蛛网式逐音高行解析。
///
/// 形状边界先导出为闭合折线（[`shape_outline_path`]，含屏幕空间 Shift 正图形约束），
/// 再交给 [`paths::path_notes`]——与 `confirm_line_tool` 的「路径轮廓 → 音符」逐行
/// 完全一致（闭合环从最左点重启、不做首尾拉伸、同起点同行只留最长）。
/// **全程不使用吸附精度**：起点/终点由边界与音高行边界（key ± 0.5）的解析交点决定。
///
/// `key_count` 为渲染音高行上限：越界行裁掉（与曲线工具 `confirm` 同口径，
/// 边界折线的首尾拉伸可能把端点推到行边界外）。
pub(super) fn outline_notes(
    spec: ShapeSpec,
    px_per_tick: f32,
    px_per_key: f32,
    key_count: i32,
) -> Vec<(f32, u16, f32)> {
    let poly: Vec<(f64, f64)> = shape_outline_path(spec, px_per_tick, px_per_key)
        .into_iter()
        .map(|(t, k)| (t as f64, k as f64))
        .collect();
    paths::path_notes(&poly, false)
        .into_iter()
        .filter(|n| n.key >= 0 && n.key < key_count)
        .map(|n| (n.start.max(0) as f32, n.key as u16, n.length() as f32))
        .collect()
}

impl Editor {
    /// 形状工具：确认（√）—— 把所有待确认图形转成音符
    ///
    /// - **描边（`filled = false`）**：走几何解析 → 逐音高行无缝变长音符
    ///   （与曲线工具轮廓同源的 [`paths::path_notes`]，不受切分档位影响）；
    /// - **填充（`filled = true`）**：每格一条 snap 长音符；开启切分档位时按行合并
    ///   连续格点后用 x 分音符的全局网格切分（与曲线工具填充同源的 [`chop_span`]）。
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
        let key_count = self.editor_state.view.key_count as i32;

        // 描边音符：(起始 tick, key, 长度, 来源图形索引)——起点/长度由几何解析决定
        let mut outline: Vec<(f32, u16, f32, usize)> = Vec::new();
        // 定长（snap）音符：填充图形 + 未开切分
        // （第三项 = 来源图形索引，供逐图形登记音符归属）
        let mut points: Vec<(f32, u16, usize)> = Vec::new();
        // 切分音符：(起始 tick, key, 长度, 来源图形索引)
        let mut chopped: Vec<(f32, u16, f32, usize)> = Vec::new();
        for (idx, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            // ① 描边：连续几何 → 逐音高行解析（无网格量化）
            if !shape.filled {
                outline.extend(
                    outline_notes(shape.spec(), px_per_tick, px_per_key, key_count)
                        .into_iter()
                        .map(|(t, k, l)| (t, k, l, idx)),
                );
                continue;
            }
            // ② 填充：覆盖格点 → 定长音符 / 按 x 分音符切分
            let cells = shape_cells(shape.spec(), true, snap, px_per_tick, px_per_key);
            match step {
                Some(s) => chopped.extend(
                    chop_cells(&cells, snap, s)
                        .into_iter()
                        .map(|(t, k, l)| (t, k, l, idx)),
                ),
                None => points.extend(cells.into_iter().map(|(t, k)| (t, k, idx))),
            }
        }
        // 整体去重（跨多个图形 + 图形内部/边界可能重合）——先到先得，保留下标最小的图形。
        // 描边音符按**精确整 tick** 去重：起点是解析交点、落在吸附网格之外，
        // 用「除以 snap 再取整」当键会把相邻两行/两段的音符误并成一条。
        let mut seen_outline: HashSet<(i64, u16)> = HashSet::new();
        outline.retain(|p| seen_outline.insert((p.0.round() as i64, p.1)));
        let mut seen: HashSet<(i64, u16)> = HashSet::new();
        points.retain(|p| seen.insert(((p.0 / snap_key).round() as i64, p.1)));
        let mut seen_chopped: HashSet<(i64, u16)> = HashSet::new();
        chopped.retain(|p| seen_chopped.insert(((p.0 / snap_key).round() as i64, p.1)));

        if outline.is_empty() && points.is_empty() && chopped.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        // 逐图形音符归属（与写入 payload 同源，仅多带来源信息）
        let mut per_shape_notes: Vec<Vec<ShapeNote>> =
            vec![Vec::new(); self.editor_state.shape_tool.shapes.len()];
        for &(tick, key, length, idx) in &outline {
            if let Some(bucket) = per_shape_notes.get_mut(idx) {
                bucket.push(ShapeNote {
                    track,
                    tick,
                    key,
                    length,
                });
            }
        }
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
        let payload: Vec<Note> = outline
            .iter()
            .map(|&(tick, key, length, _)| Note::new(tick, key, length))
            .chain(
                points
                    .iter()
                    .map(|&(tick, key, _)| Note::new(tick, key, snap)),
            )
            .chain(
                chopped
                    .iter()
                    .map(|&(tick, key, length, _)| Note::new(tick, key, length)),
            )
            .collect();
        let mut create_ops: Vec<CreateOp> = Vec::with_capacity(payload.len());
        if payload.len() <= crate::interaction::BATCH_INSERT_THRESHOLD {
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
}
