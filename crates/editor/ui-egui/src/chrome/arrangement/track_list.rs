//! 走带左侧轨道列（160px）的 egui 绘制。
//!
//! 与 iced `ui-editor/src/arrangement/track_list/draw.rs` 同口径：
//! - 行底色：选中 → 主色弱调；否则斑马纹（浅色 0.97 / 0.94，深色 0.20 / 0.15）；
//! - 未设置音轨颜色时左侧画 8px 灰块；
//! - 第一行小字：三位编号 + 标签（`Master` / `A01` / 自定义）；
//! - 第二行：音轨名；
//! - 行高 ≥ 30 时右侧画 18×18 的 M / S 按钮（主控轨不画）；
//! - 拖拽排序：被拖行叠 25% 黑遮罩，插入位置画 3px 主色横线；
//! - 右缘 1px 分隔线。
//!
//! 本阶段**只绘制不接管输入**（点击仍由 iced 处理），用于共存对照。

use egui::{Align2, Color32, CornerRadius, FontId, Rect};

use crate::theme;

use crate::chrome::{ArrangementTrackListModel, ArrangementTrackRow};

/// 未配色时的左侧色块宽度（像素）。
const BADGE_WIDTH: f32 = 8.0;
/// 文本左侧边距（像素）。
const TEXT_MARGIN: f32 = 6.0;
/// 静音/独奏按钮尺寸（像素）。
const BTN_SIZE: f32 = 18.0;
/// 静音/独奏按钮间距（像素）。
const BTN_GAP: f32 = 2.0;
/// 拖拽插入指示线厚度（像素）。
const DROP_HEIGHT: f32 = 3.0;
/// 行高低于该值时不画静音/独奏按钮与两行文字。
const DETAIL_MIN_HEIGHT: f32 = 30.0;

/// 绘制轨道列（`rect` = 160px 宽的内容区）。
pub fn show(ui: &egui::Ui, rect: Rect, model: &ArrangementTrackListModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let clipped = ui.painter().with_clip_rect(rect);
    let painter = &clipped;
    painter.rect_filled(rect, 0, theme::base_bg());

    let row_h = model.track_height.max(1.0);
    let is_light = !theme::palette().is_dark;
    let first = (model.scroll_y / row_h).floor().max(0.0) as usize;
    let visible = (rect.height() / row_h).ceil().max(0.0) as usize + 2;
    let last = (first + visible).min(model.tracks.len());

    for index in first..last {
        let Some(track) = model.tracks.get(index) else {
            continue;
        };
        let y = rect.top() + index as f32 * row_h - model.scroll_y;
        if y + row_h < rect.top() || y > rect.bottom() {
            continue;
        }
        let row = Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), row_h));

        // 行底色。
        let bg = match track.color {
            Some(color) if track.selected => blend(color, theme::selected_bg(), 0.35),
            Some(color) => color,
            // 选中：主色弱调（对应 iced \palette.primary.weak\），不是灰底，
            // 否则配合白字会糊成一片。
            None if track.selected => blend(theme::base_bg(), theme::accent(), 0.16),
            None if index % 2 == 0 => {
                if is_light {
                    Color32::from_rgb(0xf7, 0xf7, 0xf7)
                } else {
                    Color32::from_rgb(0x33, 0x33, 0x33)
                }
            }
            None => {
                if is_light {
                    Color32::from_rgb(0xf0, 0xf0, 0xf0)
                } else {
                    Color32::from_rgb(0x26, 0x26, 0x26)
                }
            }
        };
        painter.rect_filled(row, 0, bg);

        // 被拖拽行：25% 黑遮罩。
        if model.drag_active && model.drag_track == Some(track.id) {
            painter.rect_filled(row, 0, Color32::from_black_alpha(64));
        }

        // 未配色：左侧 8px 灰块。
        if track.color.is_none() {
            painter.rect_filled(
                Rect::from_min_size(row.min, egui::vec2(BADGE_WIDTH, row_h)),
                0,
                if is_light {
                    Color32::from_rgb(0x99, 0x99, 0x99)
                } else {
                    Color32::from_rgb(0x80, 0x80, 0x80)
                },
            );
        }

        let text_color = if track.selected && track.color.is_none() {
            // 对应 iced \palette.primary.strong\：深主色在浅主色底上可读。
            theme::accent_strong()
        } else if track.color.is_some() {
            contrast_text(bg)
        } else {
            theme::text()
        };

        if row_h >= DETAIL_MIN_HEIGHT {
            let small = (row_h * 0.25).clamp(8.0, 13.0);
            let text_x = rect.left() + BADGE_WIDTH + TEXT_MARGIN;
            let label = if track.conductor {
                "Master".to_string()
            } else if track.label.is_empty() {
                let port = (b'A' + (track.channel / 16).min(7)) as char;
                format!("{}{:02}", port, (track.channel % 16) + 1)
            } else {
                track.label.clone()
            };
            // 第一行：编号（左）+ 标签（右移 32）。
            painter.text(
                egui::pos2(text_x, y + row_h * 0.30),
                Align2::LEFT_CENTER,
                format!("{:03}", track.id),
                FontId::proportional(small),
                text_color,
            );
            painter.text(
                egui::pos2(text_x + 32.0, y + row_h * 0.30),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(small),
                text_color,
            );
            // 第二行：音轨名。
            let name_size = (row_h * 0.25).clamp(9.0, 13.0);
            painter.text(
                egui::pos2(text_x, y + row_h * 0.70),
                Align2::LEFT_CENTER,
                &track.name,
                FontId::proportional(name_size),
                text_color,
            );

            if !track.conductor {
                draw_buttons(painter, rect, y, row_h, track, is_light, text_color);
            }
        } else {
            // 窄行：单行编号 + 名称。
            let size = (row_h * 0.45).clamp(8.0, 14.0);
            let text_x = rect.left() + BADGE_WIDTH + TEXT_MARGIN;
            painter.text(
                egui::pos2(text_x, y + row_h * 0.5),
                Align2::LEFT_CENTER,
                format!("{:03}", track.id),
                FontId::proportional(size),
                text_color,
            );
            painter.text(
                egui::pos2(text_x + 40.0, y + row_h * 0.5),
                Align2::LEFT_CENTER,
                &track.name,
                FontId::proportional(size),
                text_color,
            );
        }
    }

    // 拖拽插入位置：3px 主色横线。
    if model.drag_active
        && let Some(index) = model.drop_index
    {
        let y = rect.top() + index as f32 * row_h - model.scroll_y;
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left(), y - DROP_HEIGHT * 0.5),
                egui::vec2(rect.width(), DROP_HEIGHT),
            ),
            0,
            theme::accent_strong(),
        );
    }

    // 右缘 1px 分隔线。
    let line = if is_light {
        Color32::from_rgba_unmultiplied(0, 0, 0, 20)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 15)
    };
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(rect.right() - 1.0, rect.top()),
            egui::vec2(1.0, rect.height()),
        ),
        0,
        line,
    );
}

/// 静音 / 独奏按钮（18×18，右对齐，间距 2，右边距 6）。
fn draw_buttons(
    painter: &egui::Painter,
    rect: Rect,
    y: f32,
    row_h: f32,
    track: &ArrangementTrackRow,
    is_light: bool,
    text_color: Color32,
) {
    let total = 2.0 * BTN_SIZE + BTN_GAP;
    let start_x = rect.right() - total - 6.0;
    let top = y + (row_h - BTN_SIZE) * 0.5;

    let idle = if is_light {
        Color32::from_rgb(0xd9, 0xd9, 0xd9)
    } else {
        Color32::from_rgb(0x40, 0x40, 0x40)
    };

    let mute_fill = if track.muted { theme::danger() } else { idle };
    let mute_rect = Rect::from_min_size(egui::pos2(start_x, top), egui::vec2(BTN_SIZE, BTN_SIZE));
    painter.rect_filled(mute_rect, CornerRadius::same(2), mute_fill);
    painter.text(
        mute_rect.center(),
        Align2::CENTER_CENTER,
        "M",
        FontId::proportional(11.0),
        if track.muted {
            Color32::WHITE
        } else {
            text_color
        },
    );

    let solo_fill = if track.soloed { theme::warning() } else { idle };
    let solo_rect = Rect::from_min_size(
        egui::pos2(start_x + BTN_SIZE + BTN_GAP, top),
        egui::vec2(BTN_SIZE, BTN_SIZE),
    );
    painter.rect_filled(solo_rect, CornerRadius::same(2), solo_fill);
    painter.text(
        solo_rect.center(),
        Align2::CENTER_CENTER,
        "S",
        FontId::proportional(11.0),
        if track.soloed {
            Color32::BLACK
        } else {
            text_color
        },
    );
}

/// 线性混合（与 iced `blend_color` 同口径）。
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// 按背景亮度选黑/白文字（对应 iced `contrast_text_color`）。
fn contrast_text(bg: Color32) -> Color32 {
    let luma = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if luma > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}
