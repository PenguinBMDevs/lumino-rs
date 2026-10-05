use ab_glyph::{Font, Point as AbPoint, PxScale, ScaleFont};
use iced_core::Point;
use lumino_editor_state::text_tool::TextToolState;

use super::font::load_font;

/// 字形光栅化超采样倍数（每个 key 列 / key 行细分为 SS×SS 子像素，提升细笔画捕获）
const SS: u32 = 4;
/// 单元格占用判定阈值：墨水子像素占比高于此值视为「有墨水」
const COVERAGE_THRESHOLD: f32 = 0.08;

/// 点是否落在矩形内
pub(super) fn point_in_rect(p: Point, r: iced_core::Rectangle) -> bool {
    p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
}

/// 字形采样网格 —— **「列/行 → 逻辑轴」的唯一权威**
///
/// 「列」= 字形 advance 方向（文字从左到右），「行」= 字形高度方向（文字从上到下）。
/// 卷帘方向决定这两轴挂到哪个逻辑轴上，**只有一种挂法能让文字在屏幕上正着读**：
///
/// | 方向 | 列（advance，屏幕向右） | 行（高度，屏幕向下） |
/// |---|---|---|
/// | 横向 | 时间 +snap（tick 递增） | 音高 −1（key 递减） |
/// | 纵向 | 音高 +1（key 递增） | 时间 −snap（tick 递减） |
///
/// **纵向为什么必须这么挂**：纵向视图的屏幕轴是 `X = key`（key 越大越靠右）、
/// `Y = tick`（tick 越大越靠上，见 `coords.rs`）。文字要正着读，其 advance 就必须沿
/// 屏幕 X、其"向下"就必须沿屏幕 Y；若照搬横向的「列→tick、行→key」，把字形按原样铺到
/// 屏幕上会得到一个**旋转 90° 且翻转**的图形（镜像），也就是"斜着/反着"读不了。
/// 轴角色换了之后，预览位图**不需要任何转置**：位图 x 就是屏幕 X、位图 y 就是屏幕 Y。
///
/// 采样分辨率同样随方向互换：横向「列 = 时间格（框内 tick 跨度 / snap）、行 = 音高格
/// （框内 key 跨度）」；纵向「列 = 音高格、行 = 时间格」——即把横向的两个格数对调。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlyphGrid {
    /// 采样列数（advance 方向）
    pub cols: usize,
    /// 采样行数（高度方向）
    pub rows: usize,
    /// 是否纵向卷帘（决定列/行各自挂到哪个逻辑轴）
    pub vertical: bool,
    /// 行 0 所在侧的时间原点：横向 = 框左缘 tick（`tick_lo`）、纵向 = 框顶 tick（`tick_hi`）
    pub tick_origin: f32,
    /// 列 0 所在的音高原点：横向 = 行 0 的 key（`key_hi`）、纵向 = 框左缘 key（`key_lo`）
    pub key_origin: u16,
    /// 每个时间格的 tick 跨度（= 音符精度 snap）
    pub tick_step: f32,
}

impl GlyphGrid {
    /// 由文本框状态 + 卷帘方向构造（轴角色的**唯一**决定点）
    pub fn from_state(tt: &TextToolState, snap: f32, vertical: bool) -> Self {
        let snap = snap.max(1.0);
        let (tick_lo, tick_hi) = tt.normalized_ticks();
        let (key_lo, key_hi) = tt.normalized_keys();
        // 框内的"时间格数 / 音高格数"（与 `TextToolState::cols/rows` 同源，不另立公式）
        let time_cells = tt.cols(snap);
        let pitch_cells = tt.rows();
        if vertical {
            // 纵向：advance(列) = 音高格、高度(行) = 时间格
            Self {
                cols: pitch_cells,
                rows: time_cells,
                vertical: true,
                tick_origin: tick_hi,
                key_origin: key_lo,
                tick_step: snap,
            }
        } else {
            // 横向：advance(列) = 时间格、高度(行) = 音高格
            Self {
                cols: time_cells,
                rows: pitch_cells,
                vertical: false,
                tick_origin: tick_lo,
                key_origin: key_hi,
                tick_step: snap,
            }
        }
    }

    /// 时间格 `k`（横向 = 列号、纵向 = 行号）的起始 tick
    ///
    /// - 横向：`tick_lo + k·snap`（列 0 在框左缘）；
    /// - 纵向：`tick_hi − (k+1)·snap`（行 0 占框**顶**那一格，避免整块字形飘出框外一格）。
    pub fn tick_of_time_cell(&self, k: usize) -> f32 {
        if self.vertical {
            (self.tick_origin - (k as f32 + 1.0) * self.tick_step).max(0.0)
        } else {
            (self.tick_origin + k as f32 * self.tick_step).max(0.0)
        }
    }

    /// 音高格 `k`（横向 = 行号、纵向 = 列号）的 key
    ///
    /// - 横向：`key_hi − k`（行 0 在屏幕上方 = 最高音）；
    /// - 纵向：`key_lo + k`（列 0 在屏幕左方 = 最低音，advance 向右升高）。
    pub fn key_of_pitch_cell(&self, k: usize) -> u16 {
        if self.vertical {
            (self.key_origin as usize + k).min(255) as u16
        } else {
            (self.key_origin as i32 - k as i32).clamp(0, 255) as u16
        }
    }

    /// 采样格 `(row, col)` → 逻辑 `(tick, key)`
    pub fn cell(&self, row: usize, col: usize) -> (f32, u16) {
        // 纵向下"时间格"是行、"音高格"是列；横向相反
        let (time_k, pitch_k) = if self.vertical {
            (row, col)
        } else {
            (col, row)
        };
        (
            self.tick_of_time_cell(time_k),
            self.key_of_pitch_cell(pitch_k),
        )
    }
}

/// 将文字光栅化为灰度位图（行优先，**行 0 = 文字顶部**，底部对齐到框底）。
///
/// 返回 `(width, height, buf)`：`width = cols * SS`、`height = rows * SS`，`buf` 按行主序存储，
/// 每像素为 0..255 的墨水墨度。预览渲染与音符采样共用同一份栅格，保证「看到的就是生成的」。
pub(crate) fn rasterize_glyph_alpha(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 {
        return None;
    }
    let font = load_font(family)?;
    let h = (rows as u32) * SS;
    let w = (cols as u32) * SS;

    // 垂直缩放：用字体 ascent+descent（不含行距）充满框高，使文字填满框且不留顶部空白。
    let unit = font.as_scaled(PxScale::from(1.0));
    let h1 = (unit.ascent() + unit.descent()).max(1e-3);
    let scale = PxScale::from(h as f32 / h1);
    let scaled = font.as_scaled(scale);

    // 第一遍：计算自然布局总推进宽度，并记录所有字形中最低墨水的 y（渲染像素，y 向下）。
    let mut total_advance = 0f32;
    let mut max_bottom = 0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        total_advance += scaled.h_advance(gid);
        if let Some(outline) =
            font.outline_glyph(gid.with_scale_and_position(scale, AbPoint::default()))
        {
            let b = outline.px_bounds();
            if b.max.y > max_bottom {
                max_bottom = b.max.y;
            }
        }
    }
    let tw = total_advance.max(1.0).ceil() as u32;

    // 底部对齐：把最低墨水（max_bottom）对齐到缓冲底（行 h）。
    let y0 = (h as f32) - max_bottom;

    // 渲染到临时缓冲（高 = h，宽 = 自然推进）
    let mut temp = vec![0u8; (h as usize) * (tw as usize)];
    let mut x_cursor = 0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, AbPoint::default());
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|px, py, alpha| {
                // px/py 为相对字形包围盒左上角的渲染像素坐标（x 向右、y 向下，无翻转）。
                // px_bounds().min 为字形在基线坐标系下的原点偏移；加 y0 实现底部对齐。
                let x = (x_cursor + px as f32 + b.min.x).round() as i32;
                let y = (py as f32 + b.min.y + y0).round() as i32;
                if x >= 0 && x < tw as i32 && y >= 0 && y < h as i32 && alpha > 0.0 {
                    temp[y as usize * tw as usize + x as usize] = (alpha * 255.0) as u8;
                }
            });
        }
        x_cursor += scaled.h_advance(gid);
    }

    // 水平拉伸到 w（高度已一致），得到铺满框的位图
    let mut buf = vec![0u8; (h as usize) * (w as usize)];
    for (r, buf_row) in buf.chunks_mut(w as usize).enumerate() {
        let temp_base = r * tw as usize;
        for (c, buf_cell) in buf_row.iter_mut().enumerate() {
            let src_x = if tw == 0 {
                0
            } else {
                ((c as f64 * tw as f64 / w as f64) as u32).min(tw - 1)
            };
            *buf_cell = temp[temp_base + src_x as usize];
        }
    }
    Some((w, h, buf))
}

/// 将文字光栅化为占用网格（rows × cols，[row][col] = 是否有墨水）
///
/// 行 0 = 文字顶部。文字**底部对齐**到框底（共用基线），并按框高度填满、按框宽度拉伸。
/// 返回 `None` 表示无字体或文字为空。
pub(crate) fn rasterize_text(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
) -> Option<Vec<Vec<bool>>> {
    let (w, _h, buf) = rasterize_glyph_alpha(text, cols, rows, family)?;

    // 由 SS×SS 子像素区域判定每个 (col,row) 是否占用
    let mut occ = vec![vec![false; cols]; rows];
    for (r, row) in occ.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let mut ink = 0u32;
            let mut total = 0u32;
            for sr in (r as u32 * SS)..((r as u32 + 1) * SS) {
                for sc in (c as u32 * SS)..((c as u32 + 1) * SS) {
                    total += 1;
                    if buf[sr as usize * w as usize + sc as usize] > 0 {
                        ink += 1;
                    }
                }
            }
            *cell = (ink as f32 / total as f32) > COVERAGE_THRESHOLD;
        }
    }
    Some(occ)
}

/// 纯函数：将占用网格按 [`GlyphGrid`] 的轴角色转换为音符列表（不依赖字体 / 画布）。
///
/// `merged = false`（正常采样）：每个有墨水的采样格生成一个音符，长度 = snap；
/// `merged = true`（key 范围合并）：**沿时间轴**把连续墨水合并为一个音符，
/// 长度 = 该段的时间跨度（横向 = 同一 key 行内的连续列；纵向 = 同一 key 列内的连续行），
/// 任意空隙断开，不合并本应分开的笔画。
///
/// 合并方向恒为**时间轴**：若照搬"按行合并"到纵向，合并方向会变成音高轴，于是
/// "音高跨度"会被当成"音符长度"写进文档（长度单位错乱），故合并方向必须由 `grid` 决定。
pub(crate) fn sample_to_notes_grid(
    occ: &[Vec<bool>],
    grid: &GlyphGrid,
    snap: f32,
    merged: bool,
) -> Vec<(f32, u16, f32)> {
    let mut notes = Vec::new();
    let rows = occ.len();
    let cols = occ.first().map_or(0, Vec::len);
    if rows == 0 || cols == 0 {
        return notes;
    }

    // 正常采样：逐格生成，长度 = snap（与方向无关）
    if !merged {
        for (r, row) in occ.iter().enumerate() {
            for (c, &on) in row.iter().enumerate() {
                if on {
                    let (tick, key) = grid.cell(r, c);
                    notes.push((tick, key, snap));
                }
            }
        }
        return notes;
    }

    // 合并采样：外层遍历**音高格**（横向 = 行、纵向 = 列），内层按**时间递增**遍历时间格。
    // 纵向的时间格是行号且 tick 随行号递减，故时间递增 = 行号倒序。
    let pitch_cells = if grid.vertical { cols } else { rows };
    let time_cells = if grid.vertical { rows } else { cols };
    for p in 0..pitch_cells {
        let mut run_start: Option<usize> = None; // 扫描顺序中首个时间格
        let mut run_last = 0usize;
        for k in 0..time_cells {
            let t = if grid.vertical { time_cells - 1 - k } else { k };
            let on = if grid.vertical {
                occ.get(t)
                    .and_then(|row| row.get(p))
                    .copied()
                    .unwrap_or(false)
            } else {
                occ.get(p)
                    .and_then(|row| row.get(t))
                    .copied()
                    .unwrap_or(false)
            };
            if on {
                if run_start.is_none() {
                    run_start = Some(t);
                }
                run_last = t;
            } else if let Some(start) = run_start.take() {
                push_time_run(&mut notes, grid, p, start, run_last, snap);
            }
        }
        if let Some(start) = run_start {
            push_time_run(&mut notes, grid, p, start, run_last, snap);
        }
    }
    notes
}

/// 时间轴合并段 `[start, last]`（扫描顺序，`start` 为段内**最早**时间格）→ 一条音符
fn push_time_run(
    notes: &mut Vec<(f32, u16, f32)>,
    grid: &GlyphGrid,
    pitch_cell: usize,
    start: usize,
    last: usize,
    snap: f32,
) {
    let len = (last.abs_diff(start) + 1) as f32 * snap;
    let key = grid.key_of_pitch_cell(pitch_cell);
    let tick = grid.tick_of_time_cell(start);
    notes.push((tick, key, len));
}
