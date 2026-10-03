//! 待确认笔画 → 音符项：**预览与 √ 写入同源**（所见即生成）
//!
//! 设计要点（BRUSH-001 补充需求）：
//! - 预览不再画"圆头粗线条折线"，而是把**覆盖格**按 `(音轨, key)` 合并成行段后画方块，
//!   与 √ 实际生成的音符（tick 对齐格线、长度 = 吸附精度、每层 1 key 高）**逐格一致**；
//! - 生成与预览共用同一套覆盖格口径（[`Editor::brush_pending_notes_by_stroke`]），
//!   杜绝"预览一套、生成一套"的口径分裂；
//! - 方块渲染天然**增量稳定**：追加采样点只会新增格子，不会移动已有格子
//!   （对照旧折线渲染：超过点数上限后等距重采样会整笔漂移 → 抖动，已由本方案取代）。
//!
//! 成本口径：覆盖计算 O(覆盖格数)，与采样点数/采样频率无关；方块数与"将要生成的音符数"
//! 同阶（合并行段后通常远小于音符数），不存在随笔画长度无界增长的重采样。

use lumino_editor_state::brush_tool::cov;
use std::collections::HashSet;

use crate::Editor;
use iced_core::{Point, Rectangle, Size};

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
    /// **唯一权威源**：√ 写入（`confirm_brush`）与画布预览都以此口径为准，
    /// 因此"所见即生成"是结构保证。无待确认笔画或粗细度为 0 时返回空。
    ///
    /// `#[cfg(test)]`：生产路径改用 [`Self::brush_pending_notes_by_stroke`]
    /// （同样口径 + 附带来源笔画，供登记图形对象），本方法保留为测试侧的
    /// 「无来源信息」基准口径。
    #[cfg(test)]
    pub(crate) fn brush_pending_notes(&self) -> Vec<BrushNoteItem> {
        self.brush_pending_notes_by_stroke()
            .into_iter()
            .map(|(_, item)| item)
            .collect()
    }

    /// 同 [`Self::brush_pending_notes`]，但**附带来源笔画索引**（用于按笔画登记图形对象）
    ///
    /// 去重口径与主入口完全一致（`seen` 键仍为 `(音轨, 格, key)`，**不含笔画索引**），
    /// 因此两者产出的音符项集合逐项相同，仅多带来源信息。
    pub(crate) fn brush_pending_notes_by_stroke(&self) -> Vec<(usize, BrushNoteItem)> {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return Vec::new();
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let mut seen: HashSet<(usize, i64, u16)> = HashSet::new();
        let mut out: Vec<(usize, BrushNoteItem)> = Vec::new();
        for (stroke_idx, stroke) in self.editor_state.brush_tool.strokes.iter().enumerate() {
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
                    out.push((
                        stroke_idx,
                        BrushNoteItem {
                            track,
                            tick_cell,
                            key,
                        },
                    ));
                }
            }
        }
        // 排序：同一 (音轨, key) 的格连续 → 预览可合并行段；写入按轨分组亦有序
        out.sort_unstable_by_key(|(_, n)| (n.track, n.key, n.tick_cell));
        out
    }

    /// 预览行段（**全量**窗口）：`(音轨, key, 起格, 止格)`（闭区间）——同一行内连续格合并
    ///
    /// 等价于 [`Self::brush_preview_runs_in_window`] 传 [`cov::CellWindow::FULL`]：
    /// 不做视口裁剪，成本 O(全部覆盖格)。用于测试、等价性对比与"需要整笔几何"的调用方；
    /// **绘制热路径请用窗口化版本**（见 §17：全量口径在长笔画下每帧 O(笔画长度) → 掉帧）。
    pub fn brush_preview_runs(&self) -> Vec<(usize, u16, i64, i64)> {
        self.brush_preview_runs_in_window(cov::CellWindow::FULL)
    }

    /// 预览行段（**视口窗口化**，绘制热路径）：只栅格化并合并窗口内的格
    ///
    /// 成本 O(段数 + 窗口内格数)，**与笔画总长度无关**。这是长笔画掉帧的修复点：
    /// 旧实现每帧对整笔做全量栅格化 + 全量行段合并，实测 2 万点笔画单帧 35.7ms、
    /// 单帧分配 70MB，而其中 **96.7% 的行段在视口外**（bench C 曲线）。
    ///
    /// 语义说明（与全量版一致，仅少了视口外的格）：
    /// - 多笔画重叠时预览可能重复画同一矩形（同色覆盖，视觉无差别）；
    /// - 相邻段共享的边界格重复出现 → 由下方"同格重复忽略"分支吸收；
    /// - √ 写入仍走 [`Editor::brush_pending_notes`]（全量 + 精确去重）——
    ///   **生成结果永远精确，不受视口影响**（这是"所见即生成"的前提）。
    ///
    /// `pub`：供画布渲染、渲染层测试与性能基准复用。
    pub fn brush_preview_runs_in_window(
        &self,
        window: cov::CellWindow,
    ) -> Vec<(usize, u16, i64, i64)> {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return Vec::new();
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let mut runs: Vec<(usize, u16, i64, i64)> = Vec::new();
        // key(0..=255) → 当前打开的 run：(音轨, 起格, 止格, runs 索引)
        let mut open: [Option<(usize, i64, i64, usize)>; 256] = [None; 256];
        // 覆盖格缓冲区跨笔画复用：每帧零分配
        //（旧实现每帧为整笔新建 Vec<格子> + HashSet 去重，2 万点 = 单帧 70MB 分配churn）
        let mut cells: Vec<cov::CoveredCell> = Vec::new();
        for stroke in &self.editor_state.brush_tool.strokes {
            let base_track = stroke.base_track;
            cells.clear();
            cov::cover_cells_in_window(&stroke.points, snap, window, &mut cells);
            for &(cell, base_key) in &cells {
                if !cell_in_document(cell) {
                    continue; // 与 `brush_pending_notes` 同源过滤（键盘列负 tick 格）
                }
                for level in 0..thickness as u16 {
                    let key = base_key.saturating_add(level);
                    if key > cov::MAX_KEY {
                        break;
                    }
                    // key 随 level 单调递增：低于窗口只能 continue，高于窗口可 break
                    if key < window.key_lo {
                        continue;
                    }
                    if key > window.key_hi {
                        break;
                    }
                    let track = self.brush_track_for_level(level as usize, base_track);
                    match open[key as usize] {
                        // 同行同轨且格连续 → 延长当前 run
                        Some((t, start, end, idx)) if t == track && end + 1 == cell => {
                            runs[idx].3 = cell;
                            open[key as usize] = Some((t, start, cell, idx));
                        }
                        // 同格重复（多笔画重叠 / 相邻段共享边界格）→ 忽略，避免零长 run
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

    /// 待确认笔画的 **wgpu 预览音符实例数据**：`(tick, key, length, color)`
    ///
    /// §18 长笔画掉帧修复：预览方块不再走 iced canvas（每次 `Frame::fill` 都要跑一遍
    /// lyon 细分 + 按色查 buffer，实测 8 万方块 = 单帧 81.6ms），改为**复用音符的
    /// wgpu 预览通路**（`NoteInstance::new_preview` + `PREVIEW_BORDER_SENTINEL`）：
    /// 每块 = 1 个实例，CPU 只做 16 字节打包，实例数由视口窗口界定 → 每帧成本与
    /// 笔画长度无关。副产品：预览与 √ 生成的音符走**同一个着色器**，
    /// "所见即生成"从"几何一致"升级为"着色一致"（只差预览分支的 70% alpha）。
    ///
    /// 口径：与画布路径共用 [`Self::brush_preview_runs_in_window`] + 同一剔除函数，
    /// 每个可见行段输出 1 个实例（矩形 = 行段 tick 跨度 × 1 key）。
    /// 颜色 = 该行段解析后音轨的显示色（`brush_track_color`，含层分配）。
    pub fn brush_preview_note_instances(&self) -> Vec<(f32, u8, f32, [f32; 4])> {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return Vec::new();
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        // 画布局部 bounds（与绘制层一致：position 是窗口坐标，只取 size）
        let canvas_bounds = Rectangle::new(
            Point::new(0.0, 0.0),
            Size::new(
                self.editor_state.canvas.size_x,
                self.editor_state.canvas.size_y,
            ),
        );
        let window = crate::grid::brush_tool_box::brush_visible_window(self, canvas_bounds);
        let runs = self.brush_preview_runs_in_window(window);
        let mut out = Vec::with_capacity(runs.len());
        // 音轨 → 颜色 记忆表：行段按 (音轨, key, 格) 有序，同一音轨会连续重复，
        // 而 `brush_track_color` 每次都要走一遍全局调色板（原子读 + 惰性静态解引用），
        // 2.5 万段实测贡献 1~2ms。音轨数很少（默认 1~3），线性查找比查调色板便宜。
        let mut colors: Vec<(usize, [f32; 4])> = Vec::with_capacity(8);
        for run in runs {
            let (track, key, cell_start, cell_end) = run;
            if crate::grid::brush_tool_box::brush_run_screen_rect(self, &window, canvas_bounds, run)
                .is_none()
            {
                continue; // 内容区外（键盘列/标尺带）
            }
            let rgba = match colors.iter().find(|(t, _)| *t == track) {
                Some((_, cached)) => *cached,
                None => {
                    let c = self.brush_track_color(track);
                    let rgba = [c.r, c.g, c.b, c.a];
                    colors.push((track, rgba));
                    rgba
                }
            };
            out.push((
                cell_start as f32 * snap,
                key.min(u8::MAX as u16) as u8,
                (cell_end - cell_start + 1) as f32 * snap,
                rgba,
            ));
        }
        out
    }
}
