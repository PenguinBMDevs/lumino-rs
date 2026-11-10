//! 走带左侧轨道列（160px）的绘制与命中。
//!
//! 绘制与 iced `ui-editor/src/arrangement/track_list/draw.rs` 同口径；
//! 命中盒由 [`row_rects`] **同一份矩形**给出，保证“看到的就是点得到的”。
//!
//! 交互语义与 iced `track_list/handlers.rs` 一致：单击选择、Shift 范围选择、
//! M/S 开关、按下注册拖拽排序候选（长按/移动激活由生产侧统一计时）。

use egui::{Align2, Color32, CornerRadius, FontId, Rect};

use crate::theme;

use super::super::{ArrangementTrackListModel, ArrangementTrackRow};
use super::ArrangementTrackAction;

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

/// 一行的命中矩形（绘制与输入共用）。
#[derive(Debug, Clone, Copy)]
pub(super) struct RowRects {
    /// 音轨 ID。
    pub(super) id: usize,
    /// 整行。
    pub(super) row: Rect,
    /// 静音按钮（主控轨为 `None`）。
    pub(super) mute: Option<Rect>,
    /// 独奏按钮（主控轨为 `None`）。
    pub(super) solo: Option<Rect>,
}

/// 按模型算出每一行的矩形（含 M/S 按钮）。
pub(super) fn row_rects(rect: Rect, model: &ArrangementTrackListModel) -> Vec<RowRects> {
    let row_h = model.track_height.max(1.0);
    model
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let y = rect.top() + index as f32 * row_h - model.scroll_y;
            let row =
                Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), row_h));
            let (mute, solo) = if track.conductor || row_h < DETAIL_MIN_HEIGHT {
                (None, None)
            } else {
                let total = 2.0 * BTN_SIZE + BTN_GAP;
                let start_x = rect.right() - total - 6.0;
                let top = y + (row_h - BTN_SIZE) * 0.5;
                (
                    Some(Rect::from_min_size(
                        egui::pos2(start_x, top),
                        egui::vec2(BTN_SIZE, BTN_SIZE),
                    )),
                    Some(Rect::from_min_size(
                        egui::pos2(start_x + BTN_SIZE + BTN_GAP, top),
                        egui::vec2(BTN_SIZE, BTN_SIZE),
                    )),
                )
            };
            RowRects {
                id: track.id,
                row,
                mute,
                solo,
            }
        })
        .collect()
}

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

    for index in 0..model.tracks.len() {
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
            // 选中：主色弱调（对应 iced `palette.primary.weak`），不是灰底，
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
            // 对应 iced `palette.primary.strong`：深主色在浅主色底上可读。
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

/// Shift 多选锚点（按窗口记住上一次点击的行下标）。
fn anchor_id(rect: Rect) -> egui::Id {
    egui::Id::unique(("lumino.arrangement.anchor", rect.left() as i32))
}

/// 处理轨道列输入，返回本帧动作。
///
/// 命中盒与绘制同源（[`row_rects`]）；`rect` 必须与绘制时相同。
pub(super) fn interact(
    ui: &egui::Ui,
    rect: Rect,
    model: &ArrangementTrackListModel,
) -> Option<ArrangementTrackAction> {
    let rows = row_rects(rect, model);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let pressed = ui.input(|i| i.pointer.any_pressed() && i.pointer.primary_pressed());
    let released = ui.input(|i| i.pointer.primary_released());
    let shift = ui.input(|i| i.modifiers.shift);
    let scrolling = ui.input(|i| i.pointer.primary_down());

    // 按下：先判 M/S，再判选择 + 注册拖拽候选。
    if pressed
        && let Some(pos) = pointer
        && rect.contains(pos)
    {
        for row in &rows {
            if let Some(btn) = row.mute
                && btn.contains(pos)
            {
                return Some(ArrangementTrackAction::ToggleMute(row.id));
            }
            if let Some(btn) = row.solo
                && btn.contains(pos)
            {
                return Some(ArrangementTrackAction::ToggleSolo(row.id));
            }
            if row.row.contains(pos) {
                let index = model.tracks.iter().position(|t| t.id == row.id)?;
                let anchor = ui
                    .ctx()
                    .data(|d| d.get_temp::<usize>(anchor_id(rect)))
                    .unwrap_or(index);
                let ids = if shift {
                    let (lo, hi) = (anchor.min(index), anchor.max(index));
                    model.tracks[lo..=hi].iter().map(|t| t.id).collect()
                } else {
                    vec![row.id]
                };
                if !shift {
                    ui.ctx().data_mut(|d| d.insert_temp(anchor_id(rect), index));
                }
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("lumino.arrangement.dragging"), row.id)
                });
                return Some(ArrangementTrackAction::Select { id: row.id, ids });
            }
        }
    }

    // 按下期间：指针位置**变化时**才上报（生产侧据此激活/更新插入位置），
    // 否则每帧一条消息会把事件队列刷爆。
    if scrolling
        && let Some(pos) = pointer
        && ui
            .ctx()
            .data(|d| d.get_temp::<usize>(egui::Id::unique("lumino.arrangement.dragging")))
            .is_some()
    {
        let last = ui
            .ctx()
            .data(|d| d.get_temp::<egui::Pos2>(egui::Id::unique("lumino.arrangement.last")));
        if last != Some(pos) {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("lumino.arrangement.last"), pos));
            return Some(ArrangementTrackAction::ReorderMoved {
                x: pos.x - rect.left(),
                y: pos.y - rect.top() + model.scroll_y,
            });
        }
    }

    // 释放：结束拖拽。
    if released
        && ui
            .ctx()
            .data_mut(|d| d.remove_temp::<usize>(egui::Id::unique("lumino.arrangement.dragging")))
            .is_some()
    {
        return Some(ArrangementTrackAction::ReorderEnded);
    }
    None
}
