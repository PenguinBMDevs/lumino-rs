//! 形状抽屉的几何常量与折行测量。
//!
//! 只做纯计算 / 只读 `ui` 的字体度量，不画像素。`show` 与各区块共用同一套
//! 折行算法，保证「算高度」与「摆按钮」不会得出两种结果。

use crate::chrome::ShapeActionButton;
use crate::text::text_width_ceil as text_width;

/// 面板内边距（生产 `column.padding(10)`）。
pub(super) const PAD: f32 = 10.0;
/// 列间距（生产 `column.spacing(6)`）。
pub(super) const GAP: f32 = 6.0;
/// 按钮行内间距（生产 `row.spacing(4)`）。
pub(super) const BTN_GAP: f32 = 4.0;
/// 库行间距（生产 `column(rows).spacing(2)`）。
pub(super) const ROW_GAP: f32 = 2.0;
/// 输入框高度（`text_input` 默认内边距 + 16px）。
pub(super) const INPUT_H: f32 = 30.0;
/// 动作按钮高度（16px 文本 + `padding([2,?])`，实测 上一代 26 逻辑）。
pub(super) const BTN_H: f32 = 26.0;
/// 库行高度（16px 文本 + 内边距，实测 上一代 30 逻辑）。
pub(super) const ROW_H: f32 = 30.0;

/// 色块单元宽（16px 文本 + 内边距，与 上一代 观感一致）。
pub(super) const SWATCH_W: f32 = 31.0;
/// 色块高。
pub(super) const SWATCH_H: f32 = 22.0;
/// 色块间距。
pub(super) const SWATCH_GAP: f32 = 2.0;

/// 每行能放下的色块数。
fn swatch_per_row(width: f32) -> usize {
    (((width + SWATCH_GAP) / (SWATCH_W + SWATCH_GAP)).floor() as usize).max(1)
}

/// 色块总行数。
fn swatch_rows(width: f32, count: usize) -> usize {
    let per = swatch_per_row(width);
    count.div_ceil(per)
}

/// 色块区总高（不含列间距）。
pub(super) fn swatch_height(width: f32, count: usize) -> f32 {
    let rows = swatch_rows(width, count) as f32;
    rows * SWATCH_H + (rows - 1.0) * SWATCH_GAP
}

/// 折行后的行数（与 `buttons_wrapped` 同一套算法）。
fn wrap_rows(ui: &egui::Ui, width: f32, buttons: &[ShapeActionButton]) -> usize {
    let left = 0.0;
    let right = width;
    let mut x = left;
    let mut rows = 1usize;
    for button_def in buttons {
        let w = button_width(ui, &button_def.label, 16.0);
        if x + w > right && x > left {
            x = left;
            rows += 1;
        }
        x += w + BTN_GAP;
    }
    rows
}

/// 折行总高（不含列间距）。
pub(super) fn wrap_height(ui: &egui::Ui, width: f32, buttons: &[ShapeActionButton]) -> f32 {
    let rows = wrap_rows(ui, width, buttons) as f32;
    rows * BTN_H + (rows - 1.0) * BTN_GAP
}

/// 按钮宽度 = 文本宽 + 左右内边距（`style.spacing.button_padding`）+ 2 边框余量。
///
/// 只用文本宽会让 egui 的按钮内边距挤占文字，出现「Ne / Sav / Renam」这类折行。
pub(super) fn button_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    let pad = ui.spacing().button_padding.x * 2.0 + 2.0;
    text_width(ui, text, size) + pad
}
