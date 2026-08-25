//! 文字工具交互与字形采样生成音符
//!
//! 交互流程（与曲线工具 / 图片转 MIDI 一致的「拉框 → 按钮确认」范式）：
//! - 选中文字工具后，在画布拖拽拉出文本框（X 向吸附音符精度、Y 向吸附 key 线）；
//! - 松手进入编辑态，画布覆盖层出现 `TextInput` 供输入文字；
//! - 框右侧 √（确认生成）/ ×（取消）/ 模式按钮（正常 / key 范围合并）；
//! - 确认时按字形占位采样生成音符。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::grid::text_tool_box::button_rects;
use crate::{EditState, Editor, Note};
use ab_glyph::{Font, FontArc, FontVec, Point as AbPoint, PxScale, ScaleFont};
use iced_core::Point;
use lumino_editor_state::note_to_event;
use lumino_editor_state::text_tool::TextToolMode;
use lumino_note_core::history::CreateOp;

/// 字形光栅化超采样倍数（每个 key 列 / key 行细分为 SS×SS 子像素，提升细笔画捕获）
const SS: u32 = 4;
/// 单元格占用判定阈值：墨水子像素占比高于此值视为「有墨水」
const COVERAGE_THRESHOLD: f32 = 0.08;

/// 字体缓存：字形解析较重，按家族名缓存 `FontArc`（内部为 `Arc`，克隆廉价），
/// 避免实时预览每帧重复读盘与解析。
static FONT_CACHE: OnceLock<Mutex<HashMap<String, FontArc>>> = OnceLock::new();

/// 点是否落在矩形内
fn point_in_rect(p: Point, r: iced_core::Rectangle) -> bool {
    p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
}

/// 加载字体（按家族名查找系统字体缓存；缺失时回退首个可用字体）
fn load_font(family: &str) -> Option<FontArc> {
    // 命中缓存直接返回（Arc 克隆廉价）
    if let Some(cached) = FONT_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(family).cloned())
    {
        return Some(cached);
    }
    use lumino_note_core::font_scanner::get_cached_fonts;
    let fonts = get_cached_fonts();
    let target = fonts
        .iter()
        .find(|f| f.name == family)
        .or_else(|| fonts.iter().find(|f| f.name.eq_ignore_ascii_case(family)))
        .or_else(|| {
            fonts
                .iter()
                .find(|f| f.name.to_lowercase().contains(&family.to_lowercase()))
        })
        .or(fonts.first())?;
    let bytes = std::fs::read(&target.path).ok()?;
    let font_vec = FontVec::try_from_vec(bytes).ok()?;
    let font = FontArc::from(font_vec);
    if let Ok(mut m) = FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        m.insert(family.to_string(), font.clone());
    }
    Some(font)
}

/// 将文字光栅化为占用网格（rows × cols，[row][col] = 是否有墨水）
///
/// 行 0 = 文字顶部。字形按框高度填充、按框宽度水平拉伸，使文字铺满文本框。
/// 返回 `None` 表示无字体或文字为空。
pub(crate) fn rasterize_text(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
) -> Option<Vec<Vec<bool>>> {
    if text.is_empty() || cols == 0 || rows == 0 {
        return None;
    }
    let font = load_font(family)?;
    let h = (rows as u32) * SS;
    let w = (cols as u32) * SS;

    // 计算「填充高度」的缩放：em 缩放到使字体行高 ≈ h
    let h1 = font.as_scaled(PxScale::from(1.0)).height().max(1e-3);
    let scale = PxScale::from(h as f32 / h1);
    let scaled = font.as_scaled(scale);

    // 先计算自然布局总推进宽度
    let mut total_advance = 0f32;
    for ch in text.chars() {
        total_advance += scaled.h_advance(font.glyph_id(ch));
    }
    let tw = total_advance.max(1.0).ceil() as u32;

    // 渲染到临时缓冲（高 = h，宽 = 自然推进）
    let mut temp = vec![0u8; (h as usize) * (tw as usize)];
    let mut x_cursor = 0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, AbPoint::default());
        if let Some(outline) = font.outline_glyph(glyph) {
            outline.draw(|px, py, alpha| {
                // 回调坐标已相对字形包围盒左上角：x 向右、y 向下（与画布一致，无翻转）。
                // px_bounds 的 min 即为原点，无需再减 ascent 或加 min 偏移。
                let x = (x_cursor + px as f32).round() as i32;
                let y = py as i32;
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

/// 纯函数：将占用网格转换为音符列表（不依赖字体 / 画布）。
///
/// `merged = false`（正常采样）：每个有墨水的 (col,row) 生成一个音符，长度 = snap；
/// `merged = true`（key 范围合并）：每个 key 行内连续有墨水的列合并为一个音符，
/// 任意空隙断开；音符长度 = 连续列数 × snap。
///
/// `key_top` 为文字顶部对应的 key（行 0 映射到 `key_top`，向下递减）。
pub(crate) fn sample_to_notes(
    occupancy: &[Vec<bool>],
    tick_lo: f32,
    key_top: i32,
    snap: f32,
    merged: bool,
) -> Vec<(f32, u16, f32)> {
    let mut notes = Vec::new();
    if merged {
        for (r, row) in occupancy.iter().enumerate() {
            let mut run_start: Option<usize> = None;
            for (c, &cell) in row.iter().enumerate() {
                match (run_start, cell) {
                    (Some(_), true) => continue,
                    (Some(start), false) => {
                        let end = c.saturating_sub(1);
                        if end >= start {
                            let key = (key_top - r as i32).clamp(0, 255) as u16;
                            let tick = tick_lo + start as f32 * snap;
                            let len = (end - start + 1) as f32 * snap;
                            notes.push((tick, key, len));
                        }
                        run_start = None;
                    }
                    (None, true) => run_start = Some(c),
                    (None, false) => {}
                }
            }
            // 行尾收尾：仍有一段未闭合的连续墨水
            if let Some(start) = run_start {
                let end = row.len().saturating_sub(1);
                if end >= start {
                    let key = (key_top - r as i32).clamp(0, 255) as u16;
                    let tick = tick_lo + start as f32 * snap;
                    let len = (end - start + 1) as f32 * snap;
                    notes.push((tick, key, len));
                }
            }
        }
    } else {
        for (r, row) in occupancy.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell {
                    let key = (key_top - r as i32).clamp(0, 255) as u16;
                    let tick = tick_lo + c as f32 * snap;
                    notes.push((tick, key, snap));
                }
            }
        }
    }
    notes
}

impl Editor {
    /// 文字工具：设置输入框文字（来自画布覆盖层 TextInput 的 on_input）
    pub(crate) fn set_text_tool_text(&mut self, text: String) {
        if self.editor_state.text_tool.active {
            self.editor_state.text_tool.text = text;
        }
    }

    /// 文字工具：切换采样模式（正常 / key 范围合并）
    pub(crate) fn toggle_text_tool_mode(&mut self) {
        let m = &mut self.editor_state.text_tool.mode;
        *m = if m.is_merged() {
            TextToolMode::Normal
        } else {
            TextToolMode::KeyRangeMerged
        };
    }

    /// 文字工具：取消并清空文本框
    pub(crate) fn cancel_text_tool(&mut self) {
        self.editor_state.text_tool.reset();
    }

    /// 文字工具：按下处理
    pub(crate) fn handle_text_tool_pressed(&mut self, pos: Point, key: u16) {
        // 已拉出框：先判按钮命中
        if self.editor_state.text_tool.active {
            if let Some(btns) = button_rects(self) {
                if point_in_rect(pos, btns.confirm) {
                    self.confirm_text_tool();
                    return;
                }
                if point_in_rect(pos, btns.cancel) {
                    self.cancel_text_tool();
                    return;
                }
                if point_in_rect(pos, btns.mode) {
                    self.toggle_text_tool_mode();
                    return;
                }
            }
            // 框内点击：保持编辑态（输入层聚焦由 iced 处理）
            if let Some((l, t, r, b)) = crate::grid::text_tool_box::box_rect_screen(self)
                && pos.x >= l
                && pos.x <= r
                && pos.y >= t
                && pos.y <= b
            {
                self.editor_state.text_tool.editing = true;
                return;
            }
            // 框外点击：取消当前框，开始拉新框
            self.cancel_text_tool();
        }

        // 新框：进入 Selecting 拖拽（Y 向吸附 key 行，与指针空白分支一致）
        let tick = self.pos_to_tick(pos);
        let view = &self.editor_state.view;
        let top_y = view.key_to_y(key);
        let bottom_y = top_y + view.zoom_y;
        self.editor_state.interaction.edit_state = EditState::Selecting {
            start_tick: tick,
            start_key: key,
            current_tick: tick,
            current_key: key,
            start_y: top_y,
            current_y: bottom_y,
        };
    }

    /// 文字工具：移动处理（拖拽中实时更新框的 current）
    pub(crate) fn handle_text_tool_moved(&mut self, pos: Point) {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
        if let EditState::Selecting {
            current_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            *current_tick = tick;
            *current_key = key;
            let view = &self.editor_state.view;
            *current_y = view.key_to_y(key) + view.zoom_y;
        }
    }

    /// 文字工具：确认生成音符（√ 按钮）
    ///
    /// 按字形占位采样：
    /// 正常模式：每个有墨水的 (col,row) 生成一个音符，长度 = 音符精度；
    /// key 范围合并模式：每个 key 行内连续有墨水的列合并为一个音符，任意空隙断开（不合并本应分开的笔画）。
    ///
    /// 成功后清空文本框与编辑历史，写入当前轨并进入撤销栈。返回是否生成了音符。
    pub(crate) fn confirm_text_tool(&mut self) -> bool {
        let tt = self.editor_state.text_tool.clone();
        if !tt.has_content() {
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        let (tick_lo, _) = tt.normalized_ticks();
        let (_key_lo, key_hi) = tt.normalized_keys();
        let cols = tt.cols(snap);
        let rows = tt.rows();
        if cols == 0 || rows == 0 {
            return false;
        }
        let occupancy = match rasterize_text(&tt.text, cols, rows, tt.font_family) {
            Some(o) => o,
            None => return false,
        };

        // 行 0 = 文字顶部 = 最高 key
        let key_top = key_hi as i32;
        let notes = sample_to_notes(&occupancy, tick_lo, key_top, snap, tt.mode.is_merged());

        if notes.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let mut create_ops = Vec::with_capacity(notes.len());
        for (tick, key, len) in notes {
            let note = Note::new(tick, key, len);
            if self.editor_state.data.insert_note(track, note.clone()) {
                create_ops.push(CreateOp {
                    track_id: track as u32,
                    note: note_to_event(note),
                });
            }
        }
        if create_ops.is_empty() {
            return false;
        }

        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        self.editor_state.text_tool.reset();
        self.mark_notes_changed();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_to_notes_normal_single_cell() {
        let occ = vec![vec![true]];
        let notes = sample_to_notes(&occ, 0.0, 64, 1920.0, false);
        assert_eq!(notes, vec![(0.0, 64, 1920.0)]);
    }

    #[test]
    fn test_sample_to_notes_normal_grid() {
        // 2x2 全占用：正常模式生成 4 个独立音符，长度均为 snap
        let occ = vec![vec![true, true], vec![true, true]];
        let notes = sample_to_notes(&occ, 100.0, 70, 480.0, false);
        // 行 0 → key 70，行 1 → key 69
        assert_eq!(notes.len(), 4);
        assert!(notes.contains(&(100.0, 70, 480.0)));
        assert!(notes.contains(&(580.0, 70, 480.0)));
        assert!(notes.contains(&(100.0, 69, 480.0)));
        assert!(notes.contains(&(580.0, 69, 480.0)));
    }

    #[test]
    fn test_sample_to_notes_merged_runs() {
        // 一行 [T, F, T, T, F]：合并为两个音符（[0],[2,3]）
        let occ = vec![vec![true, false, true, true, false]];
        let notes = sample_to_notes(&occ, 0.0, 60, 1920.0, true);
        assert_eq!(notes, vec![(0.0, 60, 1920.0), (3840.0, 60, 3840.0)]);
    }

    #[test]
    fn test_sample_to_notes_merged_gap_breaks() {
        // 相邻但有空隙的行：空隙处必须断开（不合并本应分开的笔画）
        let occ = vec![vec![true, false, true]];
        let notes = sample_to_notes(&occ, 0.0, 60, 100.0, true);
        // [0..0] len 100, [2..2] len 100
        assert_eq!(notes, vec![(0.0, 60, 100.0), (200.0, 60, 100.0)]);
    }

    #[test]
    fn test_sample_to_notes_merged_multiline_keys() {
        // 两行：key_top=65 → 行 0 = 65，行 1 = 64
        let occ = vec![vec![true, true], vec![true, false]];
        let notes = sample_to_notes(&occ, 0.0, 65, 1920.0, true);
        assert!(notes.contains(&(0.0, 65, 3840.0))); // 行0 连续
        assert!(notes.contains(&(0.0, 64, 1920.0))); // 行1 单格
        assert!(!notes.iter().any(|&(_, k, _)| k == 66));
    }

    #[test]
    fn test_sample_to_notes_empty() {
        let occ: Vec<Vec<bool>> = vec![];
        assert!(sample_to_notes(&occ, 0.0, 60, 1920.0, true).is_empty());
    }

    #[test]
    fn test_rasterize_text_produces_ink() {
        // 仅在能加载到默认字体时验证（Windows 一般有 Microsoft YaHei；CI 缺失则跳过）
        if let Some(occ) = rasterize_text("A", 8, 8, "Microsoft YaHei") {
            assert!(occ.iter().flatten().any(|&b| b), "可识别字符应产生墨水");
        }
    }

    #[test]
    fn test_rasterize_text_empty() {
        assert!(rasterize_text("", 8, 8, "Microsoft YaHei").is_none());
    }
}
