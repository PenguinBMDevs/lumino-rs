//! 遮罩参数面板（DRAW-010）。
//!
//! 入口：选中待确认 / 已 Bake 形状 → 右键「遮罩设置…」。面板锚在鼠标右下 12px，
//! 点面板外关闭。文案由生产侧传入（本 crate 不引 i18n）。
//!
//! 与 iced 原实现同口径：面板宽 250（外框 +10 内边距）、行间距 6、标签列 52、
//! 开关 11px、带宽滑杆 110 + 数值 30。

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Stroke, Vec2};

use crate::theme;

/// 面板内容宽度（生产 `PANEL_WIDTH`）。
const PANEL_W: f32 = 250.0;
/// 内边距（生产 `container(...).padding(10)`）。
const PAD: f32 = 10.0;
/// 标签列宽度（生产 `label(...).width(Fixed(52))`）。
const LABEL_W: f32 = 52.0;
/// 滑杆宽度（生产 `slider(...).width(Fixed(110))`）。
const SLIDER_W: f32 = 110.0;
/// 数值列宽度（生产 `text(...).width(Fixed(30))`）。
const VALUE_W: f32 = 30.0;

/// 区域来源（与 `lumino_core::shape::MaskShow` 一一对应，避免本 crate 依赖 core）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskShowKind {
    /// 闭合形状内部。
    Inside,
    /// 只取沿线带。
    Line,
}

/// 面板文案（生产侧格式化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskPanelLabels {
    /// 标题「遮罩」。
    pub title: String,
    /// 关闭按钮「✕」。
    pub close: String,
    /// 「启用」。
    pub enable: String,
    /// 启用开关文案「On」。
    pub enable_on: String,
    /// 「区域」。
    pub region: String,
    /// 「内部」。
    pub inside: String,
    /// 「沿线」。
    pub line: String,
    /// 「带宽」。
    pub size: String,
    /// 「取反」。
    pub invert: String,
    /// 「挖洞」。
    pub hole: String,
    /// 「自身发声」。
    pub own: String,
    /// 「边框」。
    pub own_border: String,
    /// 「应用」。
    pub apply: String,
    /// 「应用 *」（有未应用草稿）。
    pub apply_dirty: String,
    /// 「已同步」。
    pub synced: String,
    /// 「未应用」。
    pub pending: String,
    /// 应用按钮的悬停说明。
    pub apply_hint: String,
}

/// 遮罩面板模型。
#[derive(Debug, Clone, PartialEq)]
pub struct MaskPanelModel {
    /// 面板左上角（锚点 + 12，逻辑坐标）。
    pub origin: egui::Pos2,
    /// 遮罩是否已启用（`false` = 尚未写入 `MaskParams`）。
    pub enabled: bool,
    /// 区域来源。
    pub show: MaskShowKind,
    /// 带宽（keys）。
    pub size_keys: f32,
    /// 取反（挖洞）。
    pub invert: bool,
    /// 自身音符是否发声。
    pub own: bool,
    /// 目标是否已 Bake（决定是否显示应用行）。
    pub baked: bool,
    /// 已 Bake 是否存在未应用草稿。
    pub dirty: bool,
    /// 文案。
    pub labels: MaskPanelLabels,
}

/// 面板动作（与 `lumino_message::MaskPanelAction` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaskPanelAction {
    /// 关闭。
    Close,
    /// 开关遮罩。
    SetEnabled(bool),
    /// 切换区域来源。
    SetShow(MaskShowKind),
    /// 调整带宽。
    SetSize(f32),
    /// 取反。
    SetInvert(bool),
    /// 自身发声。
    SetOwn(bool),
    /// 应用草稿（已 Bake）。
    Apply,
}

/// 面板外点击关闭 + 面板本体；返回本帧动作。
///
/// `full` = 窗口逻辑矩形。面板打开期间整窗计入占用（与 iced 的全屏 backdrop
/// 同义：点面板外关闭，且不再把这一击传给下层）。
pub fn show(ui: &egui::Ui, full: Rect, model: &MaskPanelModel) -> Option<MaskPanelAction> {
    let ctx = ui.ctx().clone();
    let mut action = None;

    // 面板：`Area` 在 Foreground 层，优先于下面的 backdrop 收到点击。
    egui::Area::new(egui::Id::unique("lumino.mask.panel"))
        .order(egui::Order::Foreground)
        .fixed_pos(model.origin)
        .show(&ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let frame = egui::Frame::new()
                .fill(theme::toolbar_bg())
                .stroke(Stroke::new(1.0, theme::border()))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(PAD);
            frame.show(ui, |ui| {
                ui.set_width(PANEL_W);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&model.labels.title)
                            .size(13.0)
                            .color(theme::text()),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(&model.labels.close)
                                        .size(12.0)
                                        .color(theme::text()),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                            )
                            .clicked()
                        {
                            action = Some(MaskPanelAction::Close);
                        }
                    });
                });

                ui.horizontal(|ui| {
                    label(ui, &model.labels.enable);
                    if toggle(ui, &model.labels.enable_on, model.enabled) {
                        action = Some(MaskPanelAction::SetEnabled(!model.enabled));
                    }
                });

                ui.horizontal(|ui| {
                    label(ui, &model.labels.region);
                    if toggle(ui, &model.labels.inside, model.show == MaskShowKind::Inside) {
                        action = Some(MaskPanelAction::SetShow(MaskShowKind::Inside));
                    }
                    if toggle(ui, &model.labels.line, model.show == MaskShowKind::Line) {
                        action = Some(MaskPanelAction::SetShow(MaskShowKind::Line));
                    }
                });

                ui.horizontal(|ui| {
                    label(ui, &model.labels.size);
                    let mut size = model.size_keys;
                    let slider = ui.add_sized(
                        Vec2::new(SLIDER_W, 18.0),
                        egui::Slider::new(&mut size, 0.5..=16.0)
                            .step_by(0.5)
                            .show_value(false),
                    );
                    if slider.changed() {
                        action = Some(MaskPanelAction::SetSize(size));
                    }
                    ui.add_sized(
                        Vec2::new(VALUE_W, 16.0),
                        egui::Label::new(
                            RichText::new(format!("{:.1}", size))
                                .size(11.0)
                                .color(theme::text()),
                        ),
                    );
                });

                ui.horizontal(|ui| {
                    label(ui, &model.labels.invert);
                    if toggle(ui, &model.labels.hole, model.invert) {
                        action = Some(MaskPanelAction::SetInvert(!model.invert));
                    }
                });

                ui.horizontal(|ui| {
                    label(ui, &model.labels.own);
                    if toggle(ui, &model.labels.own_border, model.own) {
                        action = Some(MaskPanelAction::SetOwn(!model.own));
                    }
                });

                if model.baked {
                    ui.horizontal(|ui| {
                        let status = if model.dirty {
                            &model.labels.pending
                        } else {
                            &model.labels.synced
                        };
                        ui.add(
                            egui::Label::new(RichText::new(status).size(11.0).color(theme::text()))
                                .truncate(),
                        )
                        .on_hover_text(&model.labels.apply_hint);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let text = if model.dirty {
                                &model.labels.apply_dirty
                            } else {
                                &model.labels.apply
                            };
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(text).size(12.0).color(theme::text()),
                                    )
                                    .fill(theme::selected_bg())
                                    .corner_radius(4),
                                )
                                .clicked()
                            {
                                action = Some(MaskPanelAction::Apply);
                            }
                        });
                    });
                }
            });
        });

    // backdrop：点面板外关闭（面板在 Foreground 层，优先收到点击）。
    let backdrop = ui.interact(
        full,
        egui::Id::unique("lumino.mask.backdrop"),
        egui::Sense::click(),
    );
    if action.is_none() && backdrop.clicked() {
        action = Some(MaskPanelAction::Close);
    }
    // 面板打开期间整窗占用：iced 不再收到点击（与 iced 全屏 backdrop 同义）。
    crate::chrome::menu::extend_blocked(ui.ctx(), [full]);
    action
}

/// 固定宽度标签列。
fn label(ui: &mut egui::Ui, text: &str) {
    ui.add_sized(
        Vec2::new(LABEL_W, 16.0),
        egui::Label::new(RichText::new(text).size(12.0).color(theme::text())),
    );
}

/// 小开关（选中时主色底）。返回是否被点击。
fn toggle(ui: &mut egui::Ui, text: &str, selected: bool) -> bool {
    let (fill, color) = if selected {
        (theme::accent(), theme::on_accent())
    } else {
        (Color32::TRANSPARENT, theme::text())
    };
    ui.add(
        egui::Button::new(RichText::new(text).size(11.0).color(color))
            .fill(fill)
            .corner_radius(4)
            .min_size(Vec2::new(0.0, 18.0)),
    )
    .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> MaskPanelLabels {
        MaskPanelLabels {
            title: "遮罩".into(),
            close: "✕".into(),
            enable: "启用".into(),
            enable_on: "On".into(),
            region: "区域".into(),
            inside: "内部".into(),
            line: "沿线".into(),
            size: "带宽".into(),
            invert: "取反".into(),
            hole: "挖洞".into(),
            own: "自身发声".into(),
            own_border: "边框".into(),
            apply: "应用".into(),
            apply_dirty: "应用 *".into(),
            synced: "已同步".into(),
            pending: "未应用".into(),
            apply_hint: "点「应用」才写入文档".into(),
        }
    }

    fn model() -> MaskPanelModel {
        MaskPanelModel {
            origin: egui::pos2(100.0, 200.0),
            enabled: false,
            show: MaskShowKind::Inside,
            size_keys: 4.0,
            invert: false,
            own: false,
            baked: true,
            dirty: false,
            labels: labels(),
        }
    }

    #[test]
    fn model_carries_draft_state() {
        let m = model();
        assert!(!m.enabled, "未启用时不能假报 On");
        assert_eq!(m.show, MaskShowKind::Inside);
        assert!(m.baked && !m.dirty);
    }

    #[test]
    fn apply_label_switches_on_dirty() {
        let mut m = model();
        m.dirty = true;
        // 有草稿时按钮文案带星号、状态列显示「未应用」——文案由模型决定，
        // 绘制层只做选择，这里锁死选择规则。
        let text = if m.dirty {
            &m.labels.apply_dirty
        } else {
            &m.labels.apply
        };
        assert_eq!(text, "应用 *");
        let status = if m.dirty {
            &m.labels.pending
        } else {
            &m.labels.synced
        };
        assert_eq!(status, "未应用");
    }
}
