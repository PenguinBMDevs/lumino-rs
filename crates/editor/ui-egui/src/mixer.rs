//! 浮动混音台：通道条布局、视口裁剪与绘制（egui 版）。
//!
//! 与 iced `root/mixer_panel{,/strips}.rs` 同口径：
//! - 通道条宽 88、间距 8、步进 96；推子/标尺/电平表统一高 180；电平条宽 6；
//! - 主体横向滚动、按索引算 x 并做视口裁剪（外扩 2 个步进，宁多勿漏）；
//! - 面板最大宽 900，标题栏 + 主体两态（最大化/仅标题）。
//!
//! 本模块只负责「条带画在哪、点到哪」；拖拽/推子交互由调用方按
//! [`MixerHit`] 分派，消息仍走生产侧的 `TrackGainChanged` 等。

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke};

use crate::theme;

/// 通道条宽（逻辑像素）。
pub const STRIP_WIDTH: f32 = 88.0;
/// 通道条间距。
pub const STRIP_SPACING: f32 = 8.0;
/// 相邻通道条步进。
pub const STRIP_STEP: f32 = STRIP_WIDTH + STRIP_SPACING;
/// 推子 / 标尺 / 电平表高度。
pub const FADER_HEIGHT: f32 = 180.0;
/// 电平条宽。
pub const METER_WIDTH: f32 = 6.0;
/// 视口裁剪外扩。
pub const CULL_MARGIN: f32 = STRIP_STEP * 2.0;
/// 面板最大宽度。
pub const PANEL_VISIBLE_WIDTH: f32 = 900.0;
/// 标题栏高度。
pub const TITLE_H: f32 = 28.0;

/// 单条通道条数据。
#[derive(Debug, Clone, PartialEq)]
pub struct MixerStrip {
    /// 显示名（轨道名 / `Master`）。
    pub name: String,
    /// 音量 0..=1（线性）。
    pub volume: f32,
    /// 声像 -1..=1。
    pub pan: f32,
    /// 静音。
    pub muted: bool,
    /// 独奏。
    pub soloed: bool,
    /// 电平 0..=1（峰值）。
    pub meter: f32,
    /// 是否主控条。
    pub master: bool,
}

/// 混音台模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixerModel {
    /// 是否展开主体（false = 仅标题栏）。
    pub maximized: bool,
    /// 主体横向滚动（像素）。
    pub scroll_x: f32,
    /// 通道条（不含主控）。
    pub strips: Vec<MixerStrip>,
    /// 主控条（`None` = 不显示）。
    pub master: Option<MixerStrip>,
}

/// 面板几何（屏幕逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerLayout {
    /// 面板整体矩形。
    pub panel: Rect,
    /// 标题栏矩形。
    pub title: Rect,
    /// 主体矩形（最大化时有效）。
    pub body: Rect,
}

impl MixerLayout {
    /// 由锚点（面板左缘 x、底缘 y）与宽度算布局。
    pub fn new(left: f32, bottom: f32, width: f32, model: &MixerModel) -> Self {
        let w = width.clamp(200.0, PANEL_VISIBLE_WIDTH);
        let body_h = if model.maximized {
            FADER_HEIGHT + 72.0
        } else {
            0.0
        };
        let h = TITLE_H + body_h;
        let panel = Rect::from_min_size(egui::pos2(left, bottom - h), egui::vec2(w, h));
        let title = Rect::from_min_size(panel.min, egui::vec2(w, TITLE_H));
        let body = Rect::from_min_max(egui::pos2(panel.left(), title.bottom()), panel.max);
        Self { panel, title, body }
    }

    /// 第 `index` 个通道条矩形（含滚动偏移）。
    pub fn strip_rect(&self, index: usize, scroll_x: f32) -> Rect {
        Rect::from_min_size(
            egui::pos2(
                self.body.left() + 8.0 + index as f32 * STRIP_STEP - scroll_x,
                self.body.top() + 8.0,
            ),
            egui::vec2(STRIP_WIDTH, FADER_HEIGHT),
        )
    }

    /// 可见通道条下标（视口裁剪，外扩 2 个步进）。
    pub fn visible(&self, count: usize, scroll_x: f32) -> Vec<usize> {
        (0..count)
            .filter(|i| {
                let r = self.strip_rect(*i, scroll_x);
                r.right() >= self.body.left() - CULL_MARGIN
                    && r.left() <= self.body.right() + CULL_MARGIN
            })
            .collect()
    }
}

/// 命中区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerHit {
    /// 标题栏（拖拽面板）。
    Title,
    /// 第 `index` 条的推子。
    Fader(usize),
    /// 第 `index` 条的声像。
    Pan(usize),
    /// 第 `index` 条的静音。
    Mute(usize),
    /// 第 `index` 条的独奏。
    Solo(usize),
}

/// 命中测试（`pos` 为屏幕坐标）。
pub fn hit_test(pos: egui::Pos2, layout: &MixerLayout, model: &MixerModel) -> Option<MixerHit> {
    if !layout.panel.contains(pos) {
        return None;
    }
    if layout.title.contains(pos) {
        return Some(MixerHit::Title);
    }
    for index in layout.visible(model.strips.len(), model.scroll_x) {
        let strip = layout.strip_rect(index, model.scroll_x);
        if !strip.contains(pos) {
            continue;
        }
        // 推子占中间竖条，声像/静音/独奏在底部三格。
        let fader = Rect::from_min_max(
            egui::pos2(strip.left() + 16.0, strip.top() + 16.0),
            egui::pos2(strip.right() - 16.0, strip.bottom() - 48.0),
        );
        if fader.contains(pos) {
            return Some(MixerHit::Fader(index));
        }
        let row_y = strip.bottom() - 40.0;
        if pos.y >= row_y {
            let third = strip.width() / 3.0;
            let rel = (pos.x - strip.left()) / third;
            return Some(match rel as u32 {
                0 => MixerHit::Pan(index),
                1 => MixerHit::Mute(index),
                _ => MixerHit::Solo(index),
            });
        }
    }
    None
}

/// 绘制面板。
pub fn show(ui: &egui::Ui, layout: &MixerLayout, model: &MixerModel) {
    let painter = ui.painter();
    painter.rect(
        layout.panel,
        CornerRadius::same(6),
        theme::panel_bg(),
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(layout.title, CornerRadius::same(6), theme::toolbar_bg());
    painter.text(
        egui::pos2(layout.title.left() + 8.0, layout.title.center().y),
        Align2::LEFT_CENTER,
        "混音台",
        FontId::proportional(12.0),
        theme::text(),
    );
    if !model.maximized {
        return;
    }

    for index in layout.visible(model.strips.len(), model.scroll_x) {
        let Some(strip) = model.strips.get(index) else {
            continue;
        };
        let rect = layout.strip_rect(index, model.scroll_x);
        if !rect.intersects(layout.body) {
            continue;
        }
        painter.rect(
            rect,
            CornerRadius::same(4),
            theme::card_bg(),
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        // 名称。
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 8.0),
            Align2::CENTER_TOP,
            &strip.name,
            FontId::proportional(11.0),
            theme::text(),
        );
        // 推子：轨道 + 位置块。
        let track = Rect::from_min_max(
            egui::pos2(rect.center().x - 3.0, rect.top() + 24.0),
            egui::pos2(rect.center().x + 3.0, rect.bottom() - 48.0),
        );
        painter.rect(
            track,
            CornerRadius::same(2),
            theme::base_bg(),
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        let h = track.height() - 6.0;
        let knob_y = track.bottom() - 3.0 - h * strip.volume.clamp(0.0, 1.0);
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(track.left() - 5.0, knob_y - 4.0),
                egui::vec2(track.width() + 10.0, 8.0),
            ),
            CornerRadius::same(2),
            if strip.master {
                theme::accent_strong()
            } else {
                theme::accent()
            },
        );
        // 电平条（右侧）。
        let meter = Rect::from_min_max(
            egui::pos2(rect.right() - 12.0, rect.top() + 24.0),
            egui::pos2(rect.right() - 12.0 + METER_WIDTH, rect.bottom() - 48.0),
        );
        painter.rect_filled(meter, CornerRadius::same(1), theme::base_bg());
        let filled = meter.height() * strip.meter.clamp(0.0, 1.0);
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(meter.left(), meter.bottom() - filled),
                egui::vec2(METER_WIDTH, filled),
            ),
            CornerRadius::same(1),
            theme::success(),
        );
        // 底部三格：声像 / M / S。
        let row_y = rect.bottom() - 34.0;
        let third = rect.width() / 3.0;
        for (i, (label, active)) in [
            ("P", strip.pan.abs() > 0.01),
            ("M", strip.muted),
            ("S", strip.soloed),
        ]
        .into_iter()
        .enumerate()
        {
            let cell = Rect::from_min_size(
                egui::pos2(rect.left() + i as f32 * third + 2.0, row_y),
                egui::vec2(third - 4.0, 18.0),
            );
            painter.rect_filled(
                cell,
                CornerRadius::same(3),
                if active {
                    if label == "M" {
                        theme::danger()
                    } else {
                        theme::warning()
                    }
                } else {
                    theme::toolbar_bg()
                },
            );
            painter.text(
                cell.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(10.0),
                if active {
                    Color32::WHITE
                } else {
                    theme::text_muted()
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(n: usize) -> MixerModel {
        MixerModel {
            maximized: true,
            scroll_x: 0.0,
            strips: (0..n)
                .map(|i| MixerStrip {
                    name: format!("T{i}"),
                    volume: 0.5,
                    pan: 0.0,
                    muted: false,
                    soloed: false,
                    meter: 0.0,
                    master: false,
                })
                .collect(),
            master: None,
        }
    }

    #[test]
    fn layout_anchors_to_bottom() {
        let m = model(2);
        let layout = MixerLayout::new(60.0, 1000.0, 400.0, &m);
        assert!(
            (layout.panel.bottom() - 1000.0).abs() < 0.01,
            "面板贴底锚点"
        );
        assert!((layout.title.top() - layout.panel.top()).abs() < 0.01);
        assert!((layout.body.top() - (layout.panel.top() + TITLE_H)).abs() < 0.01);
    }

    #[test]
    fn strip_geometry_uses_step_and_scroll() {
        let m = model(3);
        let layout = MixerLayout::new(0.0, 500.0, 400.0, &m);
        let r0 = layout.strip_rect(0, 0.0);
        let r1 = layout.strip_rect(1, 0.0);
        assert!((r1.left() - r0.left() - STRIP_STEP).abs() < 0.01);
        let r1s = layout.strip_rect(1, 20.0);
        assert!((r1s.left() - (r1.left() - 20.0)).abs() < 0.01);
    }

    #[test]
    fn visible_culls_by_viewport_with_margin() {
        // 面板宽 200 → 主体约 200，只容 2 条；外扩 2 个步进仍能看到 0..=3。
        let m = model(10);
        let layout = MixerLayout::new(0.0, 500.0, 200.0, &m);
        let vis = layout.visible(10, 0.0);
        assert!(vis.contains(&0) && vis.contains(&1), "前两条必须可见");
        assert!(!vis.contains(&9), "远端条应被裁剪");
    }

    #[test]
    fn hit_test_maps_title_fader_and_buttons() {
        let m = model(1);
        let layout = MixerLayout::new(0.0, 500.0, 300.0, &m);
        assert_eq!(
            hit_test(layout.title.center(), &layout, &m),
            Some(MixerHit::Title)
        );
        let strip = layout.strip_rect(0, 0.0);
        assert_eq!(
            hit_test(
                egui::pos2(strip.center().x, strip.top() + 60.0),
                &layout,
                &m
            ),
            Some(MixerHit::Fader(0))
        );
        let third = strip.width() / 3.0;
        assert_eq!(
            hit_test(
                egui::pos2(strip.left() + third * 1.5, strip.bottom() - 24.0),
                &layout,
                &m
            ),
            Some(MixerHit::Mute(0))
        );
        assert_eq!(
            hit_test(
                egui::pos2(strip.left() + third * 2.5, strip.bottom() - 24.0),
                &layout,
                &m
            ),
            Some(MixerHit::Solo(0))
        );
    }
}
