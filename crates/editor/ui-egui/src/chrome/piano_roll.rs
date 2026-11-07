//! 钢琴卷帘骨架：键盘、标尺、滚动条、播放指示线。
//!
//! 真实网格线与音符由 `lumino-gfx` 的 wgpu 通道绘制；本层只复刻 iced 的
//! UI 骨架，因此中央区域保持纯底色，不伪造网格。

use egui::{Align2, Color32, FontId, Rect, Vec2};
use egui_extras::{Size, StripBuilder};

use super::ChromeState;
use crate::layout;
use crate::theme;

/// 钢琴卷帘视图参数。
#[derive(Debug, Clone, Copy)]
pub struct PianoRollView {
    /// 首个可见小节号。
    pub first_measure: u32,
    /// 每小节像素宽度。
    pub pixels_per_measure: f32,
    /// 水平滚动比例（0 = 起点，1 = 末尾）。
    pub scroll_x_ratio: f32,
    /// 垂直滚动比例。
    pub scroll_y_ratio: f32,
    /// 水平滚动条滑块占轨道长度的比例。
    pub thumb_x_ratio: f32,
    /// 垂直滚动条滑块占轨道长度的比例。
    pub thumb_y_ratio: f32,
}

impl Default for PianoRollView {
    fn default() -> Self {
        // 滑块比例取自 iced 基线（`main_chrome`）：水平 22/1300、垂直 278/874。
        Self {
            first_measure: 1,
            pixels_per_measure: 768.0,
            scroll_x_ratio: 0.0,
            scroll_y_ratio: 0.0,
            thumb_x_ratio: 22.0 / 1300.0,
            thumb_y_ratio: 278.0 / 874.0,
        }
    }
}

/// 渲染钢琴卷帘骨架。
///
/// 布局严格对齐生产 `Editor::view`：
/// `column![ row![canvas(键盘+网格), 垂直滚动条(20)], 水平滚动条(20) ]`——
/// 水平滚动条横跨整个编辑区（含键盘与垂直滚动条）。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    let view = state.piano_roll;
    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(layout::SCROLLBAR))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                StripBuilder::new(ui)
                    .size(Size::exact(layout::KEYBOARD_W))
                    .size(Size::remainder())
                    .size(Size::exact(layout::SCROLLBAR))
                    .horizontal(|mut strip| {
                        strip.cell(keyboard);
                        strip.cell(|ui| grid(ui, view));
                        strip.cell(|ui| vertical_scrollbar(ui, view));
                    });
            });
            strip.cell(|ui| horizontal_scrollbar(ui, view));
        });
}

fn keyboard(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::PANEL_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::RULER_H))
        .size(Size::remainder())
        .vertical(|mut strip| {
            strip.cell(|ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0, theme::BASE_BG);
            });
            strip.cell(keys);
        });
}

fn keys(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let visible = (rect.height() / layout::KEY_H).ceil() as u16;
    let painter = ui.painter();

    for row in 0..visible {
        let key = 127u16.saturating_sub(row);
        let y = rect.top() + f32::from(row) * layout::KEY_H;
        let key_rect = Rect::from_min_size(
            egui::pos2(rect.left(), y),
            Vec2::new(rect.width(), layout::KEY_H),
        );
        let black = matches!(key % 12, 1 | 3 | 6 | 8 | 10);
        let fill = if black {
            theme::BASE_BG
        } else {
            theme::PANEL_BG
        };
        painter.rect_filled(key_rect, 0, fill);
        // iced `grid/keyboard.rs` 用 1px `background.base` 描边整键：白键上呈
        // #272C41 的细分隔线，黑键上与底色同值不可见。
        painter.rect_stroke(
            key_rect,
            0,
            egui::Stroke::new(1.0, theme::BASE_BG),
            egui::StrokeKind::Inside,
        );
        // 音名：iced 在键盘宽度中点居中、暗色主题纯白、字号 11。
        painter.text(
            egui::pos2(rect.center().x, y + layout::KEY_H / 2.0),
            Align2::CENTER_CENTER,
            note_name(key),
            FontId::proportional(theme::font::SMALL),
            Color32::WHITE,
        );
    }
}

fn grid(ui: &mut egui::Ui, view: PianoRollView) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::RULER_H))
        .size(Size::remainder())
        .vertical(|mut strip| {
            strip.cell(|ui| ruler(ui, view));
            strip.cell(playhead);
        });
}

fn ruler(ui: &mut egui::Ui, view: PianoRollView) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    // iced 的时间标尺与网格同底色，只靠文字区分层级；文字为纯白。
    painter.rect_filled(rect, 0, theme::BASE_BG);

    let mut x = 0.0;
    let mut measure = view.first_measure;
    while x < rect.width() {
        painter.text(
            egui::pos2(rect.left() + x + 4.0, rect.top() + 4.0),
            Align2::LEFT_TOP,
            measure.to_string(),
            FontId::proportional(12.5),
            egui::Color32::WHITE,
        );
        x += view.pixels_per_measure;
        measure += 1;
    }
}

fn playhead(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);
    // iced 以 `keyboard_width` 为线中心，2px 宽 → 覆盖 [x-1, x+1)。
    let center = rect.left();
    ui.painter().rect_filled(
        Rect::from_min_size(
            egui::pos2(center - PLAYHEAD_WIDTH / 2.0, rect.top()),
            Vec2::new(PLAYHEAD_WIDTH, rect.height()),
        ),
        0,
        theme::PLAYHEAD,
    );
    // 顶部倒三角（iced `PLAYBACK_INDICATOR_TRIANGLE_SIZE = 8`，顶点朝下）。
    let half = PLAYHEAD_TRIANGLE / 2.0;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(center - half, rect.top()),
            egui::pos2(center + half, rect.top()),
            egui::pos2(center, rect.top() + PLAYHEAD_TRIANGLE),
        ],
        theme::PLAYHEAD,
        egui::Stroke::NONE,
    ));
}

fn horizontal_scrollbar(ui: &mut egui::Ui, view: PianoRollView) {
    scrollbar(ui, view.scroll_x_ratio, view.thumb_x_ratio, false);
}

fn vertical_scrollbar(ui: &mut egui::Ui, view: PianoRollView) {
    scrollbar(ui, view.scroll_y_ratio, view.thumb_y_ratio, true);
}

/// iced `ScrollbarWidget` 外观：轨道 `background.strong`，滑块 `background.weak`，
/// 滑块四周内缩 2px，无圆角。
fn scrollbar(ui: &mut egui::Ui, scroll_ratio: f32, thumb_ratio: f32, vertical: bool) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, 0, theme::SELECTED_BG);

    let track = rect.shrink(THUMB_GAP);
    let (length, offset) = if vertical {
        (track.height(), track.top())
    } else {
        (track.width(), track.left())
    };
    let thumb_len = (length * thumb_ratio).max(THUMB_MIN_SIZE).min(length);
    let start = offset + scroll_ratio.clamp(0.0, 1.0) * (length - thumb_len);

    let thumb = if vertical {
        Rect::from_min_size(
            egui::pos2(track.left(), start),
            Vec2::new(track.width(), thumb_len),
        )
    } else {
        Rect::from_min_size(
            egui::pos2(start, track.top()),
            Vec2::new(thumb_len, track.height()),
        )
    };
    painter.rect_filled(thumb, 0, theme::TOOLBAR_BG);
}

fn note_name(key: u16) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(key % 12) as usize];
    let octave = i32::from(key) / 12 - 1;
    format!("{name}{octave}")
}

/// 播放指示线宽度（与生产 `PLAYBACK_INDICATOR_WIDTH` 对齐）。
const PLAYHEAD_WIDTH: f32 = 2.0;
/// 播放指示线顶部倒三角尺寸（`PLAYBACK_INDICATOR_TRIANGLE_SIZE`）。
const PLAYHEAD_TRIANGLE: f32 = 8.0;
/// 滚动条滑块与轨道的间距（`TRACK_THUMB_GAP` / `THUMB_TRACK_EDGE_GAP`）。
const THUMB_GAP: f32 = 2.0;
/// 滚动条滑块最小长度（`THUMB_MIN_SIZE_PX`）。
const THUMB_MIN_SIZE: f32 = 20.0;

/// 中央区域底色（供外部检查）。
pub const BACKGROUND: Color32 = theme::BASE_BG;
