//! 工程设置对话框（`DialogType::ProjectSettings`）。
//!
//! 窗口使用**系统原生标题栏**（iced 侧只渲染内容），因此 egui 整窗绘制表单：
//! 24px 内边距、标题 18px、标签/正文 14px、输入框 `background.weak` 底 +
//! 1px `background.strong` 边框、圆角 4；按钮 100×34（取消 / 确定，间距 12）。
//! 表单在 `ScrollArea` 内（与生产 `scrollable(form)` 一致，按钮随内容滚动）。
//!
//! 动作只回传「字段新值 / 确认 / 取消」，合法性校验与工程写入仍由生产侧负责。

use egui::{CornerRadius, FontId, Frame, Id, Margin, RichText, ScrollArea, Stroke, Vec2};

use crate::chrome::ProjectSettingsModel;
use crate::theme;

const ACTION_ID: &str = "lumino.project-settings.action";
/// 整窗内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 区块间距：生产 `space().height(12)` + `column.spacing(4)`。
const SECTION_GAP: f32 = 12.0;
/// 输入框高度（生产 `text_input` padding [6,10]、14px 文本）。
const INPUT_H: f32 = 30.0;
/// 按钮尺寸（生产 `padding([8, 32])`、宽 100）。
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;
/// 拍号输入框宽度（生产 `Length::Fixed(60)`）。
const TS_INPUT_W: f32 = 60.0;

/// 工程设置对话框产生的动作（不依赖 iced）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSettingsAction {
    /// 项目名称变化。
    NameChanged(String),
    /// BPM 文本变化（生产侧只接受数字与小数点）。
    TempoChanged(String),
    /// 版权信息变化。
    CopyrightChanged(String),
    /// 作者变化。
    AuthorChanged(String),
    /// 拍号分子变化（生产侧只接受数字）。
    NumeratorChanged(String),
    /// 拍号分母变化。
    DenominatorChanged(String),
    /// 确定：生产侧校验 BPM / 拍号后写回工程。
    Confirm,
    /// 取消：关闭对话框。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: ProjectSettingsAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<ProjectSettingsAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<ProjectSettingsAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<ProjectSettingsAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &ProjectSettingsModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    // 自制标题栏由 iced 绘制（红绿灯 + 拖动区）：egui 只画标题栏以下的内容区，
    // 否则会把窗口控制按钮盖掉（功能削减）。
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 4.0;
    // 滚动条与生产 `scrollable` 同口径：8px 实体条（不悬浮），滑块 `strong`。
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = child.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();
    ScrollArea::vertical()
        .id_salt("lumino.project-settings.scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| form(ui, model, inner.width()));
}

fn form(ui: &mut egui::Ui, model: &ProjectSettingsModel, width: f32) {
    ui.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    ui.add_space(20.0);

    label(ui, &model.name_label);
    text_field(
        ui,
        "name",
        &model.name,
        &model.name_hint,
        width,
        ProjectSettingsAction::NameChanged,
    );

    ui.add_space(SECTION_GAP);
    label(ui, &model.bpm_label);
    bpm_row(ui, model, width);

    ui.add_space(SECTION_GAP);
    label(ui, &model.copyright_label);
    text_field(
        ui,
        "copyright",
        &model.copyright,
        &model.copyright_hint,
        width,
        ProjectSettingsAction::CopyrightChanged,
    );

    ui.add_space(SECTION_GAP);
    label(ui, &model.author_label);
    text_field(
        ui,
        "author",
        &model.author,
        &model.author_hint,
        width,
        ProjectSettingsAction::AuthorChanged,
    );

    ui.add_space(SECTION_GAP);
    label(ui, &model.time_signature_label);
    time_signature_row(ui, model);

    ui.add_space(SECTION_GAP);
    label(ui, &model.created_label);
    value(ui, &model.created);

    ui.add_space(SECTION_GAP);
    label(ui, &model.editing_time_label);
    value(ui, &model.editing_time);

    if let Some(note_count) = &model.note_count {
        ui.add_space(SECTION_GAP);
        label(ui, &note_count.label);
        value(ui, &note_count.value);
    }

    ui.add_space(24.0);
    buttons(ui, model);
}

/// 表单标签（14px 正文色）。
fn label(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(
        RichText::new(text).size(14.0).color(theme::text()),
    ));
}

/// 只读值（与标签同色，生产 `readonly_style = background.neutral.text`）。
fn value(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(
        RichText::new(text).size(14.0).color(theme::text()),
    ));
}

/// 输入框：`background.weak` 底 + 1px `background.strong` 边框、圆角 4。
fn text_field(
    ui: &mut egui::Ui,
    id: &str,
    text: &str,
    hint: &str,
    width: f32,
    action: impl FnOnce(String) -> ProjectSettingsAction,
) {
    let mut buffer = text.to_owned();
    // 输入区宽度 = 外框宽 - 左右内边距（10+10）；`desired_width(INFINITY)` 在
    // 横向布局里会撑满整行（拍号 60 宽会被拉成整宽），这里必须给确切值。
    let text_w = (width - 20.0).max(1.0);
    let response = input_frame().show(ui, |ui| {
        ui.set_min_width(text_w);
        ui.set_min_height(INPUT_H - 12.0);
        ui.push_id(("project-settings", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .hint_text(RichText::new(hint).size(14.0).color(theme::text_dim()))
                    .desired_width(text_w),
            )
        })
        .inner
    });
    if response.inner.changed() {
        push_action(ui.ctx(), action(buffer));
    }
}

/// BPM 行：输入框 + 8px 间距 + 单位「BPM」。
fn bpm_row(ui: &mut egui::Ui, model: &ProjectSettingsModel, width: f32) {
    let unit_w = 34.0;
    let input_w = (width - unit_w - 8.0).max(1.0);
    ui.horizontal(|ui| {
        text_field(
            ui,
            "bpm",
            &model.bpm,
            "",
            input_w,
            ProjectSettingsAction::TempoChanged,
        );
        ui.add_space(8.0);
        ui.add(egui::Label::new(
            RichText::new("BPM").size(14.0).color(theme::text_dim()),
        ));
    });
}

/// 拍号行：60 宽分子 + 4 + 「/」 + 4 + 60 宽分母。
fn time_signature_row(ui: &mut egui::Ui, model: &ProjectSettingsModel) {
    ui.horizontal(|ui| {
        text_field(
            ui,
            "ts-num",
            &model.numerator,
            "",
            TS_INPUT_W,
            ProjectSettingsAction::NumeratorChanged,
        );
        ui.add_space(4.0);
        ui.add(egui::Label::new(
            RichText::new("/").size(14.0).color(theme::text()),
        ));
        ui.add_space(4.0);
        text_field(
            ui,
            "ts-den",
            &model.denominator,
            "",
            TS_INPUT_W,
            ProjectSettingsAction::DenominatorChanged,
        );
    });
}

/// 底部按钮：取消（次要）+ 12 + 确定（主色）。
fn buttons(ui: &mut egui::Ui, model: &ProjectSettingsModel) {
    ui.horizontal(|ui| {
        if button(ui, &model.cancel, theme::toolbar_bg(), theme::text()).clicked() {
            push_action(ui.ctx(), ProjectSettingsAction::Cancel);
        }
        ui.add_space(BUTTON_GAP);
        if button(ui, &model.ok, theme::accent(), theme::on_accent()).clicked() {
            push_action(ui.ctx(), ProjectSettingsAction::Confirm);
        }
    });
}

fn button(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    text: egui::Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.add_sized(Vec2::new(BUTTON_W, BUTTON_H), button)
}

/// 输入框外框（生产 `text_input` 口径：`background.base` 底 + 1px `strong` 边框）。
fn input_frame() -> Frame {
    Frame::NONE
        .fill(theme::base_bg())
        .stroke(Stroke::new(1.0, theme::selected_bg()))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 6))
}
