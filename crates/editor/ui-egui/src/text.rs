//! 文本适配：单行省略与多行折行的统一入口。
//!
//! 全局约定（界面文本只有三种合法结局）：
//! 1. **单行槽位**（按钮 / 菜单项 / 列表行 / 标签）：超宽 → 尾部省略号，绝不换行；
//! 2. **说明段落**（设置页说明、对话框提示）：按可用宽度折行，并让容器高度随行数增长；
//! 3. **不设上限**（内容自适应布局）：原样绘制。
//!
//! `egui::Painter::text` 不做任何裁剪或折行，所有单行绘制都必须经由本模块给出宽度，
//! 否则长文案会直接画到相邻控件或容器之外。判断口径：宽度来自「槽位」而不是文本本身。

use std::sync::Arc;

use egui::{Align2, Color32, FontId, Galley, Pos2, Ui};

/// 省略号（与 epaint 的截断字符保持一致）。
pub const ELLIPSIS: &str = "…";

/// 单行文本的绘制宽度（不换行）。
pub fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    layout_no_wrap(ui, text, size).size().x
}

/// 单行文本的行高（用于给折行结果预留高度）。
pub fn line_height(ui: &Ui, size: f32) -> f32 {
    layout_no_wrap(ui, "国", size).size().y
}

/// 按最大宽度截断文本（尾部加省略号）；`max_width <= 0` 视为不限宽。
///
/// 二分查找最长可容纳前缀：每步只做一次排版，长文件名 / URL 不会退化成
/// 「逐字符重排」的平方级开销。
pub fn truncate_to_width(ui: &Ui, text: &str, size: f32, max_width: f32) -> String {
    if max_width <= 0.0 || text_width(ui, text, size) <= max_width {
        return text.to_owned();
    }
    if text_width(ui, ELLIPSIS, size) > max_width {
        return ELLIPSIS.to_owned();
    }

    let chars: Vec<char> = text.chars().collect();
    let (mut kept, mut rejected) = (0usize, chars.len());
    while kept < rejected {
        let mid = (kept + rejected).div_ceil(2);
        let mut candidate: String = chars[..mid].iter().collect();
        candidate.push_str(ELLIPSIS);
        if text_width(ui, &candidate, size) <= max_width {
            kept = mid;
        } else {
            rejected = mid - 1;
        }
    }

    let mut out: String = chars[..kept].iter().collect();
    out.push_str(ELLIPSIS);
    out
}

/// 折行排版（不绘制）。
pub fn layout_wrapped(
    ui: &Ui,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    ui.painter().layout(
        text.to_owned(),
        FontId::proportional(size),
        color,
        max_width.max(1.0),
    )
}

/// 折行后的高度；`max_width <= 0` 时按单行返回。
pub fn wrapped_height(ui: &Ui, text: &str, size: f32, max_width: f32) -> f32 {
    if max_width <= 0.0 {
        return line_height(ui, size);
    }
    layout_wrapped(ui, text, size, Color32::WHITE, max_width)
        .size()
        .y
}

/// 从 `pos`（左上角）开始绘制折行文本，返回实际高度。
pub fn paint_wrapped(
    ui: &Ui,
    pos: Pos2,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> f32 {
    let galley = layout_wrapped(ui, text, size, color, max_width);
    let height = galley.size().y;
    ui.painter().galley(pos, galley, color);
    height
}

/// 以 `center` 为中心（水平 + 垂直）绘制折行文本，返回实际高度。
pub fn paint_wrapped_centered_at(
    ui: &Ui,
    center: Pos2,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> f32 {
    let galley = layout_wrapped(ui, text, size, color, max_width);
    let size = galley.size();
    let pos = Pos2::new(center.x - size.x * 0.5, center.y - size.y * 0.5);
    let height = size.y;
    ui.painter().galley(pos, galley, color);
    height
}

/// 单行绘制：超宽时截断并加省略号。`max_width <= 0` 时原样绘制。
pub fn paint_truncated(
    ui: &Ui,
    pos: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) {
    let fitted = truncate_to_width(ui, text, size, max_width);
    ui.painter()
        .text(pos, align, fitted, FontId::proportional(size), color);
}

fn layout_no_wrap(ui: &Ui, text: &str, size: f32) -> Arc<Galley> {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE)
}

#[cfg(test)]
mod tests;
