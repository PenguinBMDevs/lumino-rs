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
    /// 内容总长度（像素，水平轴 `max_scroll.0`）。
    pub max_scroll_x: f32,
    /// 内容总长度（像素，垂直轴 `max_scroll.1`）。
    pub max_scroll_y: f32,
    /// 垂直滚动条的视口高度（画布高 − 标尺高）。
    pub viewport_y: f32,
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
            max_scroll_x: 1300.0,
            max_scroll_y: 854.0,
            viewport_y: 830.0,
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
    // 底层由 egui 采样 gfx 离屏纹理时（`LUMINO_EGUI_CANVAS=1`），这里不能再铺
    // 不透明底色——否则会把下面那层画布盖掉（真机实测：播放指示线消失）。
    if !super::base_texture_active(ui.ctx()) {
        ui.painter().rect_filled(rect, 0, theme::base_bg());
    }
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
    scrollbar_cells(ui, area, view, canvas_w, canvas_h);
}

/// 两条滚动条：绘制 + 拖拽（滚动 / 边缘缩放）。
fn scrollbar_cells(ui: &egui::Ui, area: Rect, view: &PianoRollView, canvas_w: f32, canvas_h: f32) {
    let vertical = crate::scrollbar::ScrollbarModel {
        rect: Rect::from_min_max(
            egui::pos2(area.left() + canvas_w, area.top()),
            egui::pos2(area.right(), area.top() + canvas_h),
        ),
        scroll: view.scroll_y,
        max_scroll: view.max_scroll_y,
        zoom: view.key_height,
        viewport: Some(view.viewport_y),
        thumb_ratio: Some(view.thumb_y_ratio),
        vertical: true,
    };
    if crate::scrollbar::show(ui, "piano-roll-y", &vertical).is_some() {
        super::menu::extend_blocked(ui.ctx(), [vertical.rect]);
    }

    let horizontal = crate::scrollbar::ScrollbarModel {
        rect: Rect::from_min_max(egui::pos2(area.left(), area.top() + canvas_h), area.max),
        scroll: view.scroll_x,
        max_scroll: view.max_scroll_x,
        zoom: view.zoom_x,
        viewport: None,
        thumb_ratio: Some(view.thumb_x_ratio),
        vertical: false,
    };
    if crate::scrollbar::show(ui, "piano-roll-x", &horizontal).is_some() {
        super::menu::extend_blocked(ui.ctx(), [horizontal.rect]);
    }
    // 两条滚动条整块占用：命中时不落到下层画布（拖拽/缩放不应同时编辑音符）。
    super::menu::extend_blocked(ui.ctx(), [vertical.rect, horizontal.rect]);
}

/// 键盘列：左上角缝隙（标尺高）+ 琴键区。
fn keyboard_cell(ui: &egui::Ui, rect: Rect, view: &PianoRollView, ruler_h: f32) {
    let painter = ui.painter();
    // iced `keyboard_background_color`（暗色 = `background.base`，亮色 = `weak`）。
    painter.rect_filled(rect, 0, theme::roll_keyboard_bg());

    let key_h = view.key_height.max(1.0);
    let max_key = f32::from(view.visible_keys.saturating_sub(1));
    let dark = theme::palette().is_dark;
    // 被标尺压住顶部的那个键：生产里角上显示它的音名（键盘列左上角）。
    let mut corner_label: Option<String> = None;
    // 键线：每个键顶边一条 1 物理像素线，与生产 gfx 的键盘一致。
    let mut key_tops: Vec<f32> = Vec::new();
    for index in 0..view.visible_keys {
        let world_y = (max_key - f32::from(index)) * key_h;
        let y = world_y - view.scroll_y + ruler_h;
        if y + key_h < ruler_h || y > rect.height() {
            continue;
        }
        if y < ruler_h && corner_label.is_none() {
            corner_label = Some(note_name(index));
        }
        let key_rect = Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + y),
            Vec2::new(rect.width(), key_h),
        );
        let black = is_key_dark(index);
        let base = if black {
            theme::roll_black_key()
        } else {
            theme::roll_white_key()
        };
        // 256 键扩展区（128..）：亮色压暗 15%、暗色提亮 15%（iced 同规则）。
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
        // 暗色生产键盘没有可见键线（gfx 只靠键色区分），亮色才有 1px 键线。
        if !dark && y + key_h > ruler_h {
            key_tops.push(key_rect.top().max(rect.top() + ruler_h));
        }
        // 音名：键盘宽度中点、主题文字色、11px（iced `KEY_LABEL_FONT_SIZE`）。
        painter.text(
            egui::pos2(rect.center().x, key_rect.center().y),
            Align2::CENTER_CENTER,
            note_name(index),
            FontId::proportional(theme::font::SMALL),
            theme::roll_text(),
        );
    }

    // 左上角与标尺同高的一条是「标尺盖住键盘」的区域：生产用它自己的底色
    // （`ruler_background_color`）覆盖，并保留被压住那个键的音名。
    if ruler_h > 0.0 {
        let corner = Rect::from_min_size(rect.min, Vec2::new(rect.width(), ruler_h));
        painter.rect_filled(corner, 0, theme::roll_ruler_bg());
        if let Some(label) = &corner_label {
            painter.text(
                corner.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(theme::font::SMALL),
                theme::roll_text(),
            );
        }
        // 键线最后画：每个键顶边压成 1 个物理像素（与生产 gfx 的整数像素对齐）。
        let ppp = ui.ctx().pixels_per_point().max(f32::EPSILON);
        let line_h = 1.0 / ppp;
        for top in key_tops {
            let snapped = (top * ppp).floor() / ppp;
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(rect.left(), snapped),
                    Vec2::new(rect.width(), line_h),
                ),
                0,
                theme::roll_key_line(),
            );
        }
    }
}

/// 标尺行：底盘 + 小节号 + 播放指示线（倒三角 + 线头）。
fn ruler_cell(ui: &egui::Ui, rect: Rect, view: &PianoRollView) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    // iced 标尺底色：暗色 = `background.base`，亮色 = `background.weakest`。
    painter.rect_filled(rect, 0, theme::roll_ruler_bg());

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
                theme::roll_text(),
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
                theme::playhead(),
            );
            let half = PLAYHEAD_TRIANGLE / 2.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center - half, rect.top()),
                    egui::pos2(center + half, rect.top()),
                    egui::pos2(center, rect.top() + PLAYHEAD_TRIANGLE),
                ],
                theme::playhead(),
                egui::Stroke::NONE,
            ));
        }
    }
}

/// iced `ScrollbarWidget` 外观：轨道 `background.strong`，滑块 `background.weak`，
/// 四周内缩 2px，无圆角。
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

/// 播放指示线宽度（`PLAYBACK_INDICATOR_WIDTH`）。
const PLAYHEAD_WIDTH: f32 = 2.0;
/// 播放指示线顶部倒三角尺寸（`PLAYBACK_INDICATOR_TRIANGLE_SIZE`）。
const PLAYHEAD_TRIANGLE: f32 = 8.0;

/// 中央区域底色（供外部检查）。
pub fn background() -> Color32 {
    theme::base_bg()
}
