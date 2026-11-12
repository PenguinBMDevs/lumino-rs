//! 纵向卷帘骨架：底部横向键盘 + 小节号 + 两条滚动条。
//!
//! 与 iced `grid::vertical_keyboard` / `vertical_labels` 同口径，仅转置：
//! - 键沿 X 排列，键宽 = `key_height`（即 `zoom_y`），键盘厚度 = `keyboard_width`；
//! - 时间轴竖直，tick 0 在键盘顶线，向上递增；小节号画在左侧 4px 处。

use egui::{Align2, FontId, Rect, Vec2};

use super::{PianoRollView, is_key_dark, note_name, tint};
use crate::scrollbar::{self, DirectedAction, ScrollbarAction, ScrollbarModel};
use crate::theme;

/// 小节号相对小节线上移量（字号 10 + 2px 间隙，与 iced 一致）。
const LABEL_LIFT: f32 = 12.0;

/// 绘制纵向骨架（键盘、小节号、边框）。
pub fn draw(ui: &egui::Ui, area: Rect, view: &PianoRollView) {
    let keyboard_h = view.keyboard_width;
    // 滚动条占外侧两条 20px：骨架只能画在画布区内，否则键盘会压住滚动条。
    let bar = crate::layout::SCROLLBAR;
    let canvas = Rect::from_min_size(
        area.min,
        Vec2::new(
            (area.width() - bar).max(0.0),
            (area.height() - bar).max(0.0),
        ),
    );
    if canvas.height() <= keyboard_h || canvas.width() <= 1.0 {
        return;
    }
    let grid_bottom = canvas.bottom() - keyboard_h;
    let painter = ui.painter();

    // 上下边框（1 物理像素，与 iced 的网格边界一致）。
    let ppp = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let line = 1.0 / ppp;
    for y in [canvas.top(), grid_bottom] {
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(canvas.left(), y),
                Vec2::new(canvas.width(), line),
            ),
            0,
            theme::roll_key_line(),
        );
    }

    // 小节号：tick 从键盘顶线向上增长。
    let zoom_x = view.zoom_x.max(f32::EPSILON);
    let span = view.ticks_per_measure.max(1.0);
    let start_tick = (view.scroll_x / zoom_x).max(0.0);
    let first = (start_tick / span).ceil().max(0.0) as u64 + 1;
    let mut measure = first;
    loop {
        let y = grid_bottom - (measure as f32 - 1.0) * span * zoom_x + view.scroll_x;
        if y < canvas.top() {
            break;
        }
        if y <= grid_bottom {
            let top = (y - LABEL_LIFT).max(canvas.top());
            painter.text(
                egui::pos2(canvas.left() + 4.0, top),
                Align2::LEFT_TOP,
                measure.to_string(),
                FontId::proportional(10.0),
                theme::roll_text(),
            );
        }
        measure += 1;
        if measure - first > 10_000 {
            break;
        }
    }

    // 底部键盘：先铺底色，再逐键填充 + 1px 边框 + 音名。
    let keyboard = Rect::from_min_size(
        egui::pos2(canvas.left(), grid_bottom),
        Vec2::new(canvas.width(), keyboard_h),
    );
    painter.rect_filled(keyboard, 0, theme::panel_bg());
    let dark = theme::palette().is_dark;
    let key_w = view.key_height.max(0.0);
    if key_w <= 0.0 {
        return;
    }
    for index in 0..view.visible_keys {
        let world_x = f32::from(index) * key_w;
        let x = canvas.left() + world_x - view.scroll_y;
        if x + key_w < canvas.left() || x > canvas.right() {
            continue;
        }
        let visible_w = key_w.min(canvas.right() - x).max(0.0);
        if visible_w < 0.5 {
            continue;
        }
        let key_rect =
            Rect::from_min_size(egui::pos2(x, grid_bottom), Vec2::new(visible_w, keyboard_h));
        let base = if is_key_dark(index) {
            theme::roll_black_key()
        } else {
            theme::roll_white_key()
        };
        // 256 键扩展区（128..）：亮色压暗 15%、暗色提亮 15%。
        let fill = if index >= 128 {
            if dark {
                tint(base, 1.15)
            } else {
                tint(base, 0.85)
            }
        } else {
            base
        };
        painter.rect_filled(key_rect, 0, fill);
        painter.rect_stroke(
            key_rect,
            0,
            egui::Stroke::new(line, theme::roll_key_line()),
            egui::StrokeKind::Inside,
        );
        if key_w >= 14.0 {
            painter.text(
                key_rect.center(),
                Align2::CENTER_CENTER,
                note_name(index),
                FontId::proportional(theme::font::SMALL),
                theme::roll_text(),
            );
        }
    }
}

/// 纵向卷帘的两条滚动条（时间轴反向）。
///
/// - 底部：音高轴（`scroll_y` / `zoom_y`，视口 = 画布宽）；
/// - 右侧：时间轴（`max_scroll_x - scroll_x` 取反，`fixed_ratio` 同样取反）。
pub fn scrollbars(
    ui: &egui::Ui,
    area: Rect,
    view: &PianoRollView,
    canvas_w: f32,
    canvas_h: f32,
) -> Option<DirectedAction> {
    let keyboard_h = view.keyboard_width.clamp(0.0, canvas_h);
    let grid_h = (canvas_h - keyboard_h).max(1.0);
    let mut out = None;

    // 时间轴（右侧竖直）：显示值与真实 scroll_x 反向。
    let inverted = (view.max_scroll_x - view.scroll_x).clamp(0.0, view.max_scroll_x);
    let time = ScrollbarModel {
        rect: Rect::from_min_max(
            egui::pos2(area.left() + canvas_w, area.top()),
            egui::pos2(area.right(), area.top() + canvas_h),
        ),
        scroll: inverted,
        max_scroll: view.max_scroll_x,
        zoom: view.zoom_x,
        viewport: Some(grid_h),
        thumb_ratio: Some(view.thumb_x_ratio),
        vertical: true,
    };
    if let Some(action) = scrollbar::show_raw(ui, "vertical-roll-time", &time) {
        let action = match action {
            ScrollbarAction::Scroll(x) => {
                ScrollbarAction::Scroll((view.max_scroll_x - x).clamp(0.0, view.max_scroll_x))
            }
            ScrollbarAction::Zoom { zoom, fixed_ratio } => ScrollbarAction::Zoom {
                zoom,
                fixed_ratio: 1.0 - fixed_ratio.clamp(0.0, 1.0),
            },
        };
        out = Some(DirectedAction {
            vertical: true,
            action,
        });
    }

    // 音高轴（底部水平）。
    let pitch = ScrollbarModel {
        rect: Rect::from_min_max(egui::pos2(area.left(), area.top() + canvas_h), area.max),
        scroll: view.scroll_y,
        max_scroll: view.max_scroll_y,
        zoom: view.key_height,
        viewport: Some(canvas_w),
        thumb_ratio: Some(view.thumb_y_ratio),
        vertical: false,
    };
    if let Some(action) = scrollbar::show_raw(ui, "vertical-roll-pitch", &pitch) {
        out = Some(DirectedAction {
            vertical: false,
            action,
        });
    }
    out
}
