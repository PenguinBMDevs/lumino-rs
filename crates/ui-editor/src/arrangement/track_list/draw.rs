//! 工程走带左侧音轨列表 —— 绘制逻辑
//!
//! 从 `track_list.rs` 抽出，控制文件行数并保持单一职责。
//! 拖拽排序时在插入位置绘制高亮横向分割线，被拖音轨叠加遮罩。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Geometry, Stroke, Text};
use lumino_ui_core::color::{blend_color, contrast_text_color};

use super::state::TrackListState;
use super::{BADGE_WIDTH, BTN_GAP, BTN_SIZE, TEXT_MARGIN, TrackListCanvas};
use crate::grid::theme::ThemeExt;
use crate::{Renderer, Theme};

/// 插入位置指示线厚度（像素）
const DROP_INDICATOR_HEIGHT: f32 = 3.0;

/// 绘制音轨列表（含拖拽排序指示）
pub(crate) fn draw(
    canvas: &TrackListCanvas,
    state: &TrackListState,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Vec<Geometry<Renderer>> {
    let mut frame = Frame::new(renderer, bounds.size());
    let canvas_w = bounds.size().width;
    let canvas_h = bounds.size().height;
    let palette = theme.extended_palette();
    let is_light = theme.is_light();

    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        Size::new(canvas_w, canvas_h),
        palette.background.base.color,
    );

    let first = (canvas.scroll_y / canvas.track_height).floor() as usize;
    let visible_count = (canvas_h / canvas.track_height).ceil() as usize + 2;
    let last = (first + visible_count).min(canvas.tracks.len());

    let dragging = state.drag_effective(canvas.drag_active);
    let drag_track_id = state.drag.as_ref().map(|d| d.track_id);

    for idx in first..last {
        let Some((track_id, name)) = canvas.tracks.get(idx) else {
            continue;
        };

        let track_y = idx as f32 * canvas.track_height - canvas.scroll_y;
        if track_y + canvas.track_height < 0.0 || track_y > canvas_h {
            continue;
        }

        let is_selected =
            state.selected_tracks.contains(track_id) || *track_id == canvas.selected_track;

        let track_color = canvas.track_colors.get(idx).copied().flatten();

        let bg_color = match track_color {
            Some(c) => {
                if is_selected {
                    blend_color(c, palette.primary.weak.color, 0.35)
                } else {
                    c
                }
            }
            None => {
                if is_selected {
                    palette.primary.weak.color
                } else if idx % 2 == 0 {
                    if is_light {
                        Color::from_rgb(0.97, 0.97, 0.97)
                    } else {
                        Color::from_rgb(0.20, 0.20, 0.20)
                    }
                } else if is_light {
                    Color::from_rgb(0.94, 0.94, 0.94)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.15)
                }
            }
        };

        frame.fill_rectangle(
            Point::new(0.0, track_y),
            Size::new(canvas_w, canvas.track_height),
            bg_color,
        );

        // 拖拽排序中：被拖音轨叠加半透明遮罩
        if dragging && drag_track_id == Some(*track_id) {
            frame.fill_rectangle(
                Point::new(0.0, track_y),
                Size::new(canvas_w, canvas.track_height),
                Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            );
        }

        // 未设置音轨颜色时，在左侧绘制默认小色块
        if track_color.is_none() {
            let badge_color = if is_light {
                Color::from_rgb(0.6, 0.6, 0.6)
            } else {
                Color::from_rgb(0.5, 0.5, 0.5)
            };
            frame.fill_rectangle(
                Point::new(0.0, track_y),
                Size::new(BADGE_WIDTH, canvas.track_height),
                badge_color,
            );
        }

        let text_color = match track_color {
            Some(_) => contrast_text_color(bg_color),
            None => {
                if is_selected {
                    palette.primary.strong.color
                } else {
                    palette.background.base.text
                }
            }
        };

        let text_x = BADGE_WIDTH + TEXT_MARGIN;
        let show_details = canvas.track_height >= 30.0;
        let track_num = format!("{:03}", track_id);

        if show_details {
            let small_size = (canvas.track_height * 0.25).clamp(8.0, 13.0);
            let label = canvas.track_labels.get(idx).cloned().unwrap_or_default();
            let label_text = if canvas.track_conductors.get(idx).copied().unwrap_or(false) {
                "Master".to_string()
            } else if label.is_empty() {
                let ch = canvas.track_channels.get(idx).copied().unwrap_or(0);
                let port = (b'A' + (ch / 16).min(7)) as char;
                format!("{}{:02}", port, (ch % 16) + 1)
            } else {
                label
            };

            frame.fill_text(Text {
                content: track_num,
                position: Point::new(text_x, track_y + canvas.track_height * 0.30),
                color: text_color,
                size: iced_core::Pixels(small_size),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                    small_size * 1.2,
                )),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Advanced,
            });

            frame.fill_text(Text {
                content: label_text,
                position: Point::new(text_x + 32.0, track_y + canvas.track_height * 0.30),
                color: text_color,
                size: iced_core::Pixels(small_size),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                    small_size * 1.2,
                )),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Advanced,
            });

            let name_size = (canvas.track_height * 0.25).clamp(9.0, 13.0);
            frame.fill_text(Text {
                content: name.clone(),
                position: Point::new(text_x, track_y + canvas.track_height * 0.70),
                color: text_color,
                size: iced_core::Pixels(name_size),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                    name_size * 1.2,
                )),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Advanced,
            });

            if !canvas.track_conductors.get(idx).copied().unwrap_or(false) {
                let muted = state.track_muted.get(idx).copied().unwrap_or(false);
                let soloed = state.track_soloed.get(idx).copied().unwrap_or(false);
                let total_btn_w = 2.0 * BTN_SIZE + BTN_GAP;
                let btn_x_start = canvas_w - total_btn_w - 6.0;
                let btn_y = track_y + (canvas.track_height - BTN_SIZE) * 0.5;

                let mute_fill = if muted {
                    palette.danger.base.color
                } else if is_light {
                    Color::from_rgb(0.85, 0.85, 0.85)
                } else {
                    Color::from_rgb(0.25, 0.25, 0.25)
                };
                frame.fill_rectangle(
                    Point::new(btn_x_start, btn_y),
                    Size::new(BTN_SIZE, BTN_SIZE),
                    mute_fill,
                );
                frame.fill_text(Text {
                    content: "M".to_string(),
                    position: Point::new(btn_x_start + BTN_SIZE * 0.5, btn_y + BTN_SIZE * 0.5),
                    color: if muted { Color::WHITE } else { text_color },
                    size: iced_core::Pixels(11.0),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(13.0)),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Center.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });

                let solo_x = btn_x_start + BTN_SIZE + BTN_GAP;
                let solo_fill = if soloed {
                    palette.warning.base.color
                } else if is_light {
                    Color::from_rgb(0.85, 0.85, 0.85)
                } else {
                    Color::from_rgb(0.25, 0.25, 0.25)
                };
                frame.fill_rectangle(
                    Point::new(solo_x, btn_y),
                    Size::new(BTN_SIZE, BTN_SIZE),
                    solo_fill,
                );
                frame.fill_text(Text {
                    content: "S".to_string(),
                    position: Point::new(solo_x + BTN_SIZE * 0.5, btn_y + BTN_SIZE * 0.5),
                    color: if soloed { Color::BLACK } else { text_color },
                    size: iced_core::Pixels(11.0),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(13.0)),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Center.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });
            }
        } else {
            let size = (canvas.track_height * 0.45).clamp(8.0, 14.0);
            frame.fill_text(Text {
                content: track_num,
                position: Point::new(text_x, track_y + canvas.track_height * 0.5),
                color: text_color,
                size: iced_core::Pixels(size),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(size * 1.2)),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Advanced,
            });
            frame.fill_text(Text {
                content: name.clone(),
                position: Point::new(text_x + 40.0, track_y + canvas.track_height * 0.5),
                color: text_color,
                size: iced_core::Pixels(size),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(size * 1.2)),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Advanced,
            });
        }
    }

    // 拖拽排序插入位置指示：两个音轨之间的高亮横向分割线
    if dragging && let Some(hover_index) = state.drag.as_ref().map(|d| d.hover_index) {
        let y = hover_index as f32 * canvas.track_height - canvas.scroll_y;
        frame.fill_rectangle(
            Point::new(0.0, y - DROP_INDICATOR_HEIGHT * 0.5),
            Size::new(canvas_w, DROP_INDICATOR_HEIGHT),
            palette.primary.strong.color,
        );
    }

    let line_color = if is_light {
        Color::from_rgba(0.0, 0.0, 0.0, 0.08)
    } else {
        Color::from_rgba(1.0, 1.0, 1.0, 0.06)
    };
    let mut lb = iced_widget::canvas::path::Builder::new();
    lb.move_to(Point::new(canvas_w - 1.0, 0.0));
    lb.line_to(Point::new(canvas_w - 1.0, canvas_h));
    frame.stroke(
        &lb.build(),
        Stroke::default().with_color(line_color).with_width(1.0),
    );

    vec![frame.into_geometry()]
}
