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
}

impl Default for PianoRollView {
    fn default() -> Self {
        Self {
            first_measure: 1,
            pixels_per_measure: 768.0,
        }
    }
}

/// 渲染钢琴卷帘骨架。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::KEYBOARD_W))
        .size(Size::remainder())
        .size(Size::exact(layout::SCROLLBAR))
        .horizontal(|mut strip| {
            strip.cell(keyboard);
            strip.cell(|ui| grid(ui, state.piano_roll));
            strip.cell(vertical_scrollbar);
        });
}

fn keyboard(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::PANEL_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::RULER_H))
        .size(Size::remainder())
        .size(Size::exact(layout::SCROLLBAR))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0, theme::BASE_BG);
            });
            strip.cell(keys);
            strip.cell(|ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0, theme::BASE_BG);
            });
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
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left(), y + layout::KEY_H - 1.0),
                Vec2::new(rect.width(), 1.0),
            ),
            0,
            theme::BORDER,
        );
        painter.text(
            egui::pos2(rect.center().x - 12.0, y + layout::KEY_H / 2.0),
            Align2::CENTER_CENTER,
            note_name(key),
            FontId::proportional(theme::font::SMALL),
            if black {
                theme::TEXT_DIM
            } else {
                theme::TEXT_MUTED
            },
        );
    }
}

fn grid(ui: &mut egui::Ui, view: PianoRollView) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::RULER_H))
        .size(Size::remainder())
        .size(Size::exact(layout::SCROLLBAR))
        .vertical(|mut strip| {
            strip.cell(|ui| ruler(ui, view));
            strip.cell(playhead);
            strip.cell(horizontal_scrollbar);
        });
}

fn ruler(ui: &mut egui::Ui, view: PianoRollView) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    // iced 的时间标尺与网格同底色，只靠文字区分层级。
    painter.rect_filled(rect, 0, theme::BASE_BG);

    let mut x = 0.0;
    let mut measure = view.first_measure;
    while x < rect.width() {
        painter.text(
            egui::pos2(rect.left() + x + 4.0, rect.top() + 4.0),
            Align2::LEFT_TOP,
            measure.to_string(),
            FontId::proportional(12.5),
            theme::TEXT_MUTED,
        );
        x += view.pixels_per_measure;
        measure += 1;
    }
}

fn playhead(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);
    let x = rect.left() + 1.0;
    ui.painter().rect_filled(
        Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(PLAYHEAD_WIDTH, rect.height()),
        ),
        0,
        theme::DANGER,
    );
}

fn horizontal_scrollbar(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TOOLBAR_BG);
    // 内容不足一屏时 thumb 占满整条（与 iced 基线一致）。
    let thumb = rect.shrink2(Vec2::new(0.0, 0.0));
    ui.painter()
        .rect_filled(thumb, egui::CornerRadius::same(10), theme::SELECTED_BG);
}

fn vertical_scrollbar(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TOOLBAR_BG);
    let thumb = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width(), (rect.height() * 0.62).max(20.0)),
    );
    ui.painter()
        .rect_filled(thumb, egui::CornerRadius::same(10), theme::SELECTED_BG);
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

/// 中央区域底色（供外部检查）。
pub const BACKGROUND: Color32 = theme::BASE_BG;
