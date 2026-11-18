use ab_glyph::{Font, Point as AbPoint, PxScale, ScaleFont};
use lumino_core::TextAlign;
use lumino_editor_state::text_tool::TextToolState;
use lumino_ui_core::geom::Point;

use super::font::load_font;
use super::scale::{LEGACY_SS, RasterScale};

/// 旧兼容路径的单元格占用阈值（墨水子像素占比高于此值视为「有墨水」）
const COVERAGE_THRESHOLD: f32 = 0.08;

/// 单元格占用判定口径
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum InkRule {
    /// 旧口径：格内墨水子像素的**面积**占比
    ///
    /// `inclusive` 保留两条旧路径的历史差异：LegacyBox 用 `>`、Parametric 用 `>=`。
    Area {
        /// 阈值（0..=1）
        threshold: f32,
        /// 是否含等号
        inclusive: bool,
    },
    /// 新口径：**key 轴覆盖率**——音高轴上多少比例的子像素行（纵向为列）落在字形内
    ///
    /// 这才是 `TextParams::threshold` 的语义：非正方形采样下面积覆盖率会被稀释
    /// 约 `ss_x/ss_y` 倍，默认 50% 会把细笔画整格判空。
    KeyAxis {
        /// 阈值（0..=1）
        threshold: f32,
    },
}

/// 点是否落在矩形内
pub(super) fn point_in_rect(p: Point, r: lumino_ui_core::geom::Rectangle) -> bool {
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
///
/// **每个格的像素宽高不在这里**：那是 [`RasterScale`] 的职责（屏幕比例权威）。
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

/// 将文字光栅化为灰度位图（行优先，**行 0 = 文字顶部**）。
///
/// 返回 `(width, height, buf)`：尺寸由 [`RasterScale`] 决定，`buf` 按行主序存储，
/// 每像素为 0..255 的墨水墨度。预览渲染与音符采样共用同一份栅格，保证「看到的就是生成的」。
///
/// `scale.legacy` 为真时走旧路径（`LEGACY_SS` 方格 + 填满框 + 横向拉伸）；
/// 否则按屏幕比例采样，并在宽度放不下时**等比缩小**（不再拉伸字形）。
pub(crate) fn rasterize_glyph_alpha(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
    scale: &RasterScale,
    align: TextAlign,
) -> Option<(u32, u32, Vec<u8>)> {
    let font = load_font(family)?;
    if scale.legacy {
        return rasterize_glyph_alpha_with_font(text, cols, rows, &font);
    }
    rasterize_glyph_alpha_letterbox(text, cols, rows, &font, scale, align)
}

/// 旧兼容路径：按框高填满、再横向拉伸到框宽（`LEGACY_SS` 方格）。
///
/// 与 [`rasterize_glyph_alpha`] 同一实现，但字体由调用方注入。
///
/// 生产路径仍按字体族加载；测试用固定字体做像素 golden，避免系统字体差异。
pub(crate) fn rasterize_glyph_alpha_with_font(
    text: &str,
    cols: usize,
    rows: usize,
    font: &ab_glyph::FontArc,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 {
        return None;
    }
    let h = (rows as u32) * LEGACY_SS;
    let w = (cols as u32) * LEGACY_SS;

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

/// 新路径：按屏幕比例采样 + **等比 letterbox**（宽度放不下时整体缩小，不拉伸字形）。
///
/// 字号先按框高（buffer 高）换算，再乘 `fit = min(1, 框宽 / 自然推进宽)`；
/// 缩小时底部基线仍贴框底，水平位置按 [`TextAlign`] 决定。
///
/// **单行**：与旧 LegacyBox 同口径（输入框是单行 `TextEdit`）；多行请走 Parametric。
pub(crate) fn rasterize_glyph_alpha_letterbox(
    text: &str,
    cols: usize,
    rows: usize,
    font: &ab_glyph::FontArc,
    scale: &RasterScale,
    align: TextAlign,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 {
        return None;
    }
    let w = scale.width(cols);
    let h = scale.height(rows);
    if w == 0 || h == 0 {
        return None;
    }

    // 1) 先按框高定字号：拟合高度必须取**墨迹高度**（所有字形 ink bbox 的并集），
    //    不能用 ascent+descent——Noto 的 ascent+descent 小于 CJK 墨迹高度，
    //    按行高缩放会把中文上下裁掉（英文没有下伸部分时不易察觉）。
    let unit = font.as_scaled(PxScale::from(1.0));
    let reference = 1000.0;
    let mut ink_min_y = f32::MAX;
    let mut ink_max_y = f32::MIN;
    for ch in text.chars() {
        if let Some(outline) = font.outline_glyph(
            font.glyph_id(ch)
                .with_scale_and_position(PxScale::from(reference), AbPoint::default()),
        ) {
            let b = outline.px_bounds();
            ink_min_y = ink_min_y.min(b.min.y);
            ink_max_y = ink_max_y.max(b.max.y);
        }
    }
    let ink_h = if ink_max_y > ink_min_y {
        (ink_max_y - ink_min_y) / reference
    } else {
        (unit.ascent() + unit.descent()).max(1e-3)
    };
    let fill = PxScale::from(h as f32 / ink_h.max(1e-3));
    let fill_scaled = font.as_scaled(fill);
    let natural_w: f32 = text
        .chars()
        .map(|ch| fill_scaled.h_advance(font.glyph_id(ch)))
        .sum::<f32>()
        .max(1.0);

    // 2) 等比 letterbox：只有放不下才缩，缩的是**整个字形**而不是横向拉伸
    let fit = if natural_w > w as f32 {
        w as f32 / natural_w
    } else {
        1.0
    };
    let px = PxScale::from(fill.y * fit);
    let scaled = font.as_scaled(px);
    let text_w = natural_w * fit;
    let x0 = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (w as f32 - text_w) * 0.5,
        TextAlign::Right => w as f32 - text_w,
    };

    // 3) 底部对齐：把最低墨水对齐到缓冲底
    let mut max_bottom = 0f32;
    for ch in text.chars() {
        if let Some(outline) = font.outline_glyph(
            font.glyph_id(ch)
                .with_scale_and_position(px, AbPoint::default()),
        ) {
            let b = outline.px_bounds();
            if b.max.y > max_bottom {
                max_bottom = b.max.y;
            }
        }
    }
    let baseline_y = h as f32 - max_bottom;

    let mut buf = vec![0u8; (w as usize) * (h as usize)];
    let mut x_cursor = x0;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(outline) =
            font.outline_glyph(gid.with_scale_and_position(px, AbPoint::default()))
        {
            let b = outline.px_bounds();
            outline.draw(|gx, gy, alpha| {
                let x = (x_cursor + gx as f32 + b.min.x).round() as i32;
                let y = (gy as f32 + b.min.y + baseline_y).round() as i32;
                if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 && alpha > 0.0 {
                    let idx = y as usize * w as usize + x as usize;
                    buf[idx] = buf[idx].max((alpha * 255.0) as u8);
                }
            });
        }
        x_cursor += scaled.h_advance(gid);
    }
    Some((w, h, buf))
}

/// 将文字光栅化为占用网格（rows × cols，`[row][col] = 是否有墨水`）
///
/// 行 0 = 文字顶部。返回 `None` 表示无字体或文字为空。
pub(crate) fn rasterize_text(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
    scale: &RasterScale,
    align: TextAlign,
) -> Option<Vec<Vec<bool>>> {
    let (w, _h, buf) = rasterize_glyph_alpha(text, cols, rows, family, scale, align)?;
    debug_assert_eq!(
        w,
        scale.width(cols),
        "灰度图宽度必须与 RasterScale 一致（占用判定的像素坐标依赖它）"
    );
    let rule = if scale.legacy {
        // 旧路径：面积占比、严格大于（逐字节兼容历史行为）
        InkRule::Area {
            threshold: COVERAGE_THRESHOLD,
            inclusive: false,
        }
    } else {
        // 新路径：key 轴覆盖率；LegacyBox 新路径阈值仍固定 8%
        InkRule::KeyAxis {
            threshold: COVERAGE_THRESHOLD,
        }
    };
    Some(alpha_to_occupancy(&buf, cols, rows, scale, rule))
}

/// 灰度图 → 占用网格（纯函数，供阈值回归测试直接调用）
///
/// 采样格 `(row, col)` 的像素区间由 [`RasterScale`] 唯一给出；判定口径见 [`InkRule`]。
pub(super) fn alpha_to_occupancy(
    buf: &[u8],
    cols: usize,
    rows: usize,
    scale: &RasterScale,
    rule: InkRule,
) -> Vec<Vec<bool>> {
    let width = scale.width(cols) as usize;
    let mut occ = vec![vec![false; cols]; rows];
    if cols == 0 || rows == 0 || width == 0 {
        return occ;
    }
    let threshold = match rule {
        InkRule::Area { threshold, .. } | InkRule::KeyAxis { threshold } => {
            threshold.clamp(0.0, 1.0)
        }
    };
    // 纵向 key 轴覆盖率要按列统计；其余口径不需要这份 scratch
    let track_cols = matches!(rule, InkRule::KeyAxis { .. }) && scale.vertical;
    let mut col_ink: Vec<bool> = Vec::new();

    for (r, row) in occ.iter_mut().enumerate() {
        let (y0, y1) = scale.y_range(r, rows);
        for (c, cell) in row.iter_mut().enumerate() {
            let (x0, x1) = scale.x_range(c, cols);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            if track_cols {
                col_ink.clear();
                col_ink.resize((x1 - x0) as usize, false);
            }
            let mut ink = 0u32;
            let mut covered_rows = 0u32;
            for y in y0..y1 {
                let mut row_ink = false;
                for x in x0..x1 {
                    let idx = y as usize * width + x as usize;
                    if buf.get(idx).copied().unwrap_or(0) > 0 {
                        ink += 1;
                        row_ink = true;
                        if track_cols {
                            col_ink[(x - x0) as usize] = true;
                        }
                    }
                }
                if row_ink {
                    covered_rows += 1;
                }
            }
            if ink == 0 {
                continue;
            }
            *cell = match rule {
                InkRule::Area { inclusive, .. } => {
                    let total = f64::from((x1 - x0) * (y1 - y0));
                    let coverage = f64::from(ink) / total;
                    if inclusive {
                        coverage >= f64::from(threshold)
                    } else {
                        coverage > f64::from(threshold)
                    }
                }
                InkRule::KeyAxis { .. } => {
                    let (covered, axis_len) = if scale.vertical {
                        (col_ink.iter().filter(|on| **on).count() as u32, x1 - x0)
                    } else {
                        (covered_rows, y1 - y0)
                    };
                    let coverage = f64::from(covered) / f64::from(axis_len.max(1));
                    coverage >= f64::from(threshold)
                }
            };
        }
    }
    occ
}
