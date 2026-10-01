//! 待确认笔画 → 音符项：**预览与 √ 写入同源**（所见即生成）
//!
//! 设计要点（BRUSH-001 补充需求）：
//! - 预览不再画"圆头粗线条折线"，而是把**覆盖格**按 `(音轨, key)` 合并成行段后画方块，
//!   与 √ 实际生成的音符（tick 对齐格线、长度 = 吸附精度、每层 1 key 高）**逐格一致**；
//! - 生成与预览共用同一个 [`Editor::brush_pending_notes`]，杜绝"预览一套、生成一套"的口径分裂；
//! - 方块渲染天然**增量稳定**：追加采样点只会新增格子，不会移动已有格子
//!   （对照旧折线渲染：超过点数上限后等距重采样会整笔漂移 → 抖动，已由本方案取代）。
//!
//! 成本口径：覆盖计算 O(覆盖格数)，与采样点数/采样频率无关；方块数与"将要生成的音符数"
//! 同阶（合并行段后通常远小于音符数），不存在随笔画长度无界增长的重采样。

use lumino_editor_state::brush_tool::cov;
use std::collections::HashSet;

use crate::Editor;

/// 待生成音符项（逻辑坐标：格索引 + key，非屏幕坐标）
///
/// `tick_cell` 为 `floor(tick / snap)` 的格索引；实际 tick = `tick_cell * snap`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrushNoteItem {
    /// 目标音轨（doc 索引，按层分配解析后）
    pub track: usize,
    /// tick 格索引
    pub tick_cell: i64,
    /// 音符 key（已含层偏移）
    pub key: u16,
}

/// 覆盖格是否落在**合法文档范围**内（`tick_cell >= 0`）
///
/// 负 tick 格只可能来自"在钢琴键盘列上落笔"（`pos_to_tick` 在键盘列投影出负 tick）。
/// 经真实输入**当前不可达**——`handle_pressed` 的 `is_inside_canvas` 守卫在横向已
/// 拒绝 `x < keyboard_width`【证据：`interaction/pressed.rs:16`、`rendering.rs:337`】，
/// 纵向 tick 由 `(grid_bottom - y + scroll_x) / zoom_x` 保证 `>= scroll_x/zoom_x`。
///
/// 但这里仍显式过滤（**纵深防御**，不是修线上 bug）：负 tick 格既不可见（键盘
/// 覆盖层遮住），`NoteEvent.start_tick` 又是 `u32`——`(-240.0) as u32` **饱和成 0**，
/// 一旦将来有绕过输入守卫的笔画来源（历史快照/反序列化/程序化构造），
/// 就会在 tick 0 凭空生成幽灵音符。预览与生成共用本判定，锁死"看不见 ⇒ 不生成"。
pub(crate) fn cell_in_document(tick_cell: i64) -> bool {
    tick_cell >= 0
}

impl Editor {
    /// 待确认笔画 → 待生成音符项（去重后按 `(音轨, key, 格)` 升序）
    ///
    /// **唯一权威源**：√ 写入与画布预览都调用本方法，因此"所见即生成"是结构保证。
    /// 无待确认笔画或粗细度为 0 时返回空。
    pub(crate) fn brush_pending_notes(&self) -> Vec<BrushNoteItem> {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return Vec::new();
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let mut seen: HashSet<(usize, i64, u16)> = HashSet::new();
        let mut out: Vec<BrushNoteItem> = Vec::new();
        for stroke in &self.editor_state.brush_tool.strokes {
            let base_track = stroke.base_track;
            let cells = stroke.covered_cells(snap);
            let mut expanded = Vec::with_capacity(cells.len() * thickness as usize);
            cov::expand_layers(&cells, thickness, &mut expanded);
            for ((tick_cell, key), level) in expanded {
                if !cell_in_document(tick_cell) {
                    continue; // 键盘列投影出的负 tick 格：不可见，也不得饱和成 tick 0 幽灵音符
                }
                let track = self.brush_track_for_level(level as usize, base_track);
                if seen.insert((track, tick_cell, key)) {
                    out.push(BrushNoteItem {
                        track,
                        tick_cell,
                        key,
                    });
                }
            }
        }
        // 排序：同一 (音轨, key) 的格连续 → 预览可合并行段；写入按轨分组亦有序
        out.sort_unstable_by_key(|n| (n.track, n.key, n.tick_cell));
        out
    }

    /// 预览行段：`(音轨, key, 起格, 止格)`（闭区间）——同一行内连续格合并
    ///
    /// **每帧热路径**：单遍扫描覆盖格 × 层，用 256 槽直接索引"当前打开的 run"，
    /// 无全局排序、无 HashSet 去重（对照：早期实现每帧对 34 万项去重+排序，
    /// 实测 70ms/帧 → 本实现 ~1ms/帧，详见 bench）。
    ///
    /// 语义说明：多笔画重叠时预览可能重复画同一矩形（同色覆盖，视觉无差别），
    /// 而 √ 写入走 [`Editor::brush_pending_notes`] 做精确去重——**生成结果永远精确**，
    /// 预览在最坏情况下只是重复填充同色像素。
    ///
    /// `pub`：供画布渲染、渲染层测试与性能基准复用。
    pub fn brush_preview_runs(&self) -> Vec<(usize, u16, i64, i64)> {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return Vec::new();
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let mut runs: Vec<(usize, u16, i64, i64)> = Vec::new();
        // key(0..=255) → 当前打开的 run：(音轨, 起格, 止格, runs 索引)
        let mut open: [Option<(usize, i64, i64, usize)>; 256] = [None; 256];
        for stroke in &self.editor_state.brush_tool.strokes {
            let base_track = stroke.base_track;
            for (cell, base_key) in stroke.covered_cells(snap) {
                if !cell_in_document(cell) {
                    continue; // 与 `brush_pending_notes` 同源过滤（键盘列负 tick 格）
                }
                for level in 0..thickness as u16 {
                    let key = base_key.saturating_add(level);
                    if key > cov::MAX_KEY {
                        break;
                    }
                    let track = self.brush_track_for_level(level as usize, base_track);
                    match open[key as usize] {
                        // 同行同轨且格连续 → 延长当前 run
                        Some((t, start, end, idx)) if t == track && end + 1 == cell => {
                            runs[idx].3 = cell;
                            open[key as usize] = Some((t, start, cell, idx));
                        }
                        // 同格重复（多笔画重叠）→ 忽略，避免零长 run
                        Some((t, _, end, _)) if t == track && end == cell => {}
                        // 其余情况 → 开新 run（跨越其他 key 后同 key 再现会被拆段，
                        // 视觉是同一行同色，拆段只增加极少矩形数，不丢格）
                        _ => {
                            let idx = runs.len();
                            runs.push((track, key, cell, cell));
                            open[key as usize] = Some((track, cell, cell, idx));
                        }
                    }
                }
            }
        }
        runs
    }
}
