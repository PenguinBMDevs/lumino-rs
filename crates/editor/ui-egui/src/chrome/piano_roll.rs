//! 钢琴卷帘骨架：键盘、标尺、滚动条。
//!
//! 真实网格线与音符由 `lumino-gfx` 的 wgpu 通道绘制，因此生产叠加层只接管
//! **纯 UI 骨架**（键盘列 / 标尺行 / 两条滚动条），中央网格区域完全不绘制。

use egui::{Align2, Color32, FontId, Rect, Vec2};

use super::ChromeState;
use crate::theme;

/// 钢琴卷帘视图参数。
#[derive(Debug, Clone, Copy)]
pub struct PianoRollView {
    /// 键盘列宽度（生产 `view.keyboard_width`）。
    pub keyboard_width: f32,
    /// 标尺高度（生产 `view.ruler_height`）。
    pub ruler_height: f32,
    /// 每个琴键的高度（生产 `view.zoom_y`）。
    pub key_height: f32,
    /// 垂直滚动偏移（像素）。
    pub scroll_y: f32,
    /// 可见琴键数（生产 `view.visible_key_count`）。
    pub visible_keys: u16,
    /// 水平缩放（tick → 像素）。
    pub zoom_x: f32,
    /// 水平滚动偏移（像素）。
    pub scroll_x: f32,
    /// 每四分音符脉冲数。
    pub ppq: u16,
    /// 每小节 tick 数（由拍号换算）。
    pub ticks_per_measure: f32,
    /// 播放指示线相对网格左边缘的偏移（像素）。
    pub playhead_offset: f32,
    /// 水平滚动条：滚动比例（0 = 起点，1 = 末尾）。
    pub scroll_x_ratio: f32,
    /// 垂直滚动条：滚动比例。
    pub scroll_y_ratio: f32,
    /// 水平滚动条滑块占轨道长度的比例。
    pub thumb_x_ratio: f32,
    /// 垂直滚动条滑块占轨道长度的比例。
    pub thumb_y_ratio: f32,
    /// 卷帘区域相对视口的左内缩（左侧栏宽度）。
    pub left_inset: f32,
    /// 卷帘区域相对视口的上内缩（标题栏 + 工具栏）。
    pub top_inset: f32,
    /// 卷帘区域相对视口的右内缩（右侧栏宽度）。
    pub right_inset: f32,
    /// 卷帘区域相对视口的下内缩（状态栏 + 力度面板）。
    pub bottom_inset: f32,
}

impl Default for PianoRollView {
    fn default() -> Self {
        // 数值取自 iced 基线（`main_chrome`，1600×1000 @1x）：
        // 键盘 120、标尺 24、琴键 20、128 键、每小节 768px、
        // 水平滑块 22/1300、垂直滑块 279/854。
        Self {
            keyboard_width: 120.0,
            ruler_height: 24.0,
            key_height: 20.0,
            scroll_y: 0.0,
            visible_keys: 128,
            zoom_x: 0.1,
            scroll_x: 0.0,
            ppq: 1920,
            ticks_per_measure: 7680.0,
            playhead_offset: 0.0,
            scroll_x_ratio: 0.0,
            scroll_y_ratio: 0.0,
            thumb_x_ratio: 22.0 / 1300.0,
            thumb_y_ratio: 279.0 / 854.0,
            left_inset: 0.0,
            top_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
        }
    }
}

impl PianoRollView {
    /// 每小节像素宽度。
    pub fn bar_pixels(&self) -> f32 {
        (self.ticks_per_measure * self.zoom_x).max(1.0)
    }

    /// 叠加层占用的区域（视口 → 卷帘）。
    pub fn area(&self, viewport: Rect) -> Rect {
        Rect::from_min_max(
            egui::pos2(
                viewport.left() + self.left_inset,
                viewport.top() + self.top_inset,
            ),
            egui::pos2(
                (viewport.right() - self.right_inset).max(viewport.left()),
                (viewport.bottom() - self.bottom_inset).max(viewport.top()),
            ),
        )
    }
}

/// 场景入口：铺满底色后绘制骨架（生产由 wgpu 绘制的网格区域在这里只是底色）。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);
    let view = state.piano_roll;
    draw_chrome(ui, &view);
}

/// 钢琴卷帘骨架的可见部分：键盘列 + 标尺 + 滚动条。
///
/// 中央网格区域**不绘制**——生产里那里是 `lumino-gfx` 的 wgpu 输出。
pub fn draw_chrome(ui: &egui::Ui, view: &PianoRollView) {
    let area = ui.max_rect();
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let bar = crate::layout::SCROLLBAR;
    let canvas_w = (area.width() - bar).max(0.0);
    let canvas_h = (area.height() - bar).max(0.0);
    let keyboard_w = view.keyboard_width.clamp(0.0, canvas_w);
    let ruler_h = view.ruler_height.clamp(0.0, canvas_h);

    keyboard_cell(
        ui,
        Rect::from_min_size(area.min, Vec2::new(keyboard_w, canvas_h)),
        view,
        ruler_h,
    );
    ruler_cell(
        ui,
        Rect::from_min_max(
            egui::pos2(area.left() + keyboard_w, area.top()),
            egui::pos2(area.left() + canvas_w, area.top() + ruler_h),
        ),
        view,
    );
    scrollbar(
        ui,
        Rect::from_min_max(
            egui::pos2(area.left() + canvas_w, area.top()),
            egui::pos2(area.right(), area.top() + canvas_h),
        ),
        view.scroll_y_ratio,
        view.thumb_y_ratio,
        true,
    );
    scrollbar(
        ui,
        Rect::from_min_max(egui::pos2(area.left(), area.top() + canvas_h), area.max),
        view.scroll_x_ratio,
        view.thumb_x_ratio,
        false,
    );
}

/// 键盘列：左上角缝隙（标尺高）+ 琴键区。
fn keyboard_cell(ui: &egui::Ui, rect: Rect, view: &PianoRollView, ruler_h: f32) {
    let painter = ui.painter();
    // iced `keyboard_background_color`（暗色 = `background.base`）。
    painter.rect_filled(rect, 0, theme::BASE_BG);

    let key_h = view.key_height.max(1.0);
    let max_key = f32::from(view.visible_keys.saturating_sub(1));
    for index in 0..view.visible_keys {
        let world_y = (max_key - f32::from(index)) * key_h;
        let y = world_y - view.scroll_y + ruler_h;
        if y + key_h < ruler_h || y > rect.height() {
            continue;
        }
        let key_rect = Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + y),
            Vec2::new(rect.width(), key_h),
        );
        let black = is_key_dark(index);
        let base = if black {
            theme::BASE_BG
        } else {
            theme::PANEL_BG
        };
        // 256 键扩展区（128..）在暗色下提亮 15%（iced 同规则）。
        let fill = if index >= 128 { tint(base, 1.15) } else { base };
        painter.rect_filled(key_rect, 0, fill);
        // iced 的 1px 描边是**居中**画在按键矩形边上的（`canvas::Frame::stroke`）：
        // 顶/底边各把一小半落在键内，形成 `#272C41` 的 50% 混合分隔线。egui 的
        // `rect_stroke` 在这里会被相邻按键的填充覆盖掉底边，因此直接按 iced 的
        // 合成结果画两条 50% 混合线。
        let edge = mix(fill, theme::BASE_BG, 0.5);
        painter.rect_stroke(
            key_rect,
            0,
            egui::Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        // 音名：键盘宽度中点、纯白、11px（iced `KEY_LABEL_FONT_SIZE`）。
        painter.text(
            egui::pos2(rect.center().x, key_rect.center().y),
            Align2::CENTER_CENTER,
            note_name(index),
            FontId::proportional(theme::font::SMALL),
            Color32::WHITE,
        );
    }
}

/// 标尺行：底盘 + 小节号 + 播放指示线（倒三角 + 线头）。
fn ruler_cell(ui: &egui::Ui, rect: Rect, view: &PianoRollView) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    // iced 标尺底色 = `background.base`；边框与刻度线同为 `background.base`
    // （暗色下不可见），因此只复刻底色与文字。
    painter.rect_filled(rect, 0, theme::BASE_BG);

    let bar = view.bar_pixels();
    let scroll_ticks = view.scroll_x / view.zoom_x.max(f32::EPSILON);
    let first = (scroll_ticks / view.ticks_per_measure.max(1.0))
        .floor()
        .max(0.0) as u64
        + 1;
    let mut measure = first;
    loop {
        // 用 f64 计算，避免大 tick/大滚动时 f32 精度把小节号位置带偏。
        let x = (measure as f64 - 1.0) * f64::from(bar) - f64::from(view.scroll_x);
        if x > f64::from(rect.width()) {
            break;
        }
        if x >= 0.0 {
            painter.text(
                egui::pos2(rect.left() + x as f32 + 4.0, rect.top() + 4.0),
                Align2::LEFT_TOP,
                measure.to_string(),
                FontId::proportional(12.5),
                Color32::WHITE,
            );
        }
        measure += 1;
        if measure - first > 10_000 {
            break;
        }
    }

    // 播放指示线：仅当落在网格范围内（iced `playback_indicator` 同判据）。
    if view.playhead_offset >= 0.0 {
        let center = rect.left() + view.playhead_offset;
        if center <= rect.right() {
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(center - PLAYHEAD_WIDTH / 2.0, rect.top()),
                    Vec2::new(PLAYHEAD_WIDTH, rect.height()),
                ),
                0,
                theme::PLAYHEAD,
            );
            let half = PLAYHEAD_TRIANGLE / 2.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center - half, rect.top()),
                    egui::pos2(center + half, rect.top()),
                    egui::pos2(center, rect.top() + PLAYHEAD_TRIANGLE),
                ],
                theme::PLAYHEAD,
                egui::Stroke::NONE,
            ));
        }
    }
}

/// iced `ScrollbarWidget` 外观：轨道 `background.strong`，滑块 `background.weak`，
/// 四周内缩 2px，无圆角。
fn scrollbar(ui: &egui::Ui, rect: Rect, scroll_ratio: f32, thumb_ratio: f32, vertical: bool) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
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

/// 是否黑键（iced `is_key_dark`）。
fn is_key_dark(key: u16) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

/// 音名（iced `note_name`：MIDI 0 = C-1）。
fn note_name(key: u16) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(key % 12) as usize];
    let octave = i32::from(key) / 12 - 1;
    format!("{name}{octave}")
}

/// 按比例提亮/压暗颜色（保留 alpha）。
fn tint(color: Color32, factor: f32) -> Color32 {
    let scale = |value: u8| ((f32::from(value) * factor).min(255.0)).round() as u8;
    Color32::from_rgba_premultiplied(
        scale(color.r()),
        scale(color.g()),
        scale(color.b()),
        color.a(),
    )
}

/// 线性混合两个不透明颜色（`t` = `to` 占比）。
fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        blend(from.r(), to.r()),
        blend(from.g(), to.g()),
        blend(from.b(), to.b()),
    )
}

/// 播放指示线宽度（`PLAYBACK_INDICATOR_WIDTH`）。
const PLAYHEAD_WIDTH: f32 = 2.0;
/// 播放指示线顶部倒三角尺寸（`PLAYBACK_INDICATOR_TRIANGLE_SIZE`）。
const PLAYHEAD_TRIANGLE: f32 = 8.0;
/// 滚动条滑块与轨道的间距（`TRACK_THUMB_GAP` / `THUMB_TRACK_EDGE_GAP`）。
const THUMB_GAP: f32 = 2.0;
/// 滚动条滑块最小长度（`THUMB_MIN_SIZE_PX`）。
const THUMB_MIN_SIZE: f32 = 20.0;

/// 中央区域底色（供外部检查）。
pub const BACKGROUND: Color32 = theme::BASE_BG;
