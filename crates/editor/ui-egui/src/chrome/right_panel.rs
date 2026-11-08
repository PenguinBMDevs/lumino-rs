//! 右侧栏面板内容（当前接管「图片转 MIDI」）。
//!
//! 几何对齐生产 `right_sidebar/view/i2m_panel.rs`：
//! - 内容内边距 8、列间距 8；标题 14px、配置小节 12px、输入框字号 12；
//! - 输入框 42×22（`padding [3,6]`、`background.weak` 底、1px `background.strong`
//!   边框、圆角 4）；调色板下拉 118 宽；
//! - 「选择图片文件」与「转换为 MIDI」为标准按钮（`padding 6`、圆角 4、
//!   `background.weak` 底、悬停 `background.base` / 主色）。
//!
//! 面板内容整块进入占用列表（拖拽手柄除外），点击不会再落到下层 iced 的同名
//! 控件；滚动由本面板自己的 `ScrollArea` 处理。

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, Vec2,
};

use super::model::{I2mControlModel, I2mField, I2mPanelModel, I2mRowModel, RightSidebarPanelModel};
use crate::theme;

mod hits;
pub use hits::{button_rect, clear_panel_rect, field_rect, panel_rect};
pub(crate) use hits::{clear_rects, record_rect, set_panel_rect};

const ACTION_ID: &str = "lumino.right-sidebar.action";

const PAD: f32 = 8.0;
const GAP: f32 = 8.0;
const ROW_GAP: f32 = 4.0;
const HEADER_H: f32 = 20.0;
const SECTION_H: f32 = 18.0;
const ROW_H: f32 = 22.0;
const BUTTON_H: f32 = 28.0;
const INPUT_W: f32 = 42.0;
const COMBO_W: f32 = 118.0;

/// 右侧栏面板产生的动作（不依赖 iced）。
#[derive(Debug, Clone, PartialEq)]
pub enum RightSidebarAction {
    /// 点击「选择图片文件」。
    SelectImageFile,
    /// 配置文本变化（原样回传，生产负责合法性校验）。
    I2mConfigTextChanged {
        /// 字段。
        field: I2mField,
        /// 新文本。
        text: String,
    },
    /// 调色板算法切换。
    I2mPaletteChanged(usize),
    /// 点击「转换为 MIDI」。
    ConvertClicked,
}

/// 记录本帧要回传给生产的动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: RightSidebarAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<RightSidebarAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<RightSidebarAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<RightSidebarAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 内容高度（供 ScrollArea 预留；配置行数固定，按模型计算）。
fn content_height(panel: &I2mPanelModel) -> f32 {
    let rows = panel.rows.len() as f32;
    let convert = panel
        .convert
        .as_ref()
        .map(|_| GAP + BUTTON_H)
        .unwrap_or(0.0);
    PAD * 2.0
        + HEADER_H
        + GAP
        + BUTTON_H
        + GAP
        + SECTION_H
        + ROW_GAP
        + rows * ROW_H
        + rows.max(1.0) * ROW_GAP
        + convert
}

/// 绘制面板内容（`rect` = 手柄右侧的内容区）。
pub fn show(ui: &mut egui::Ui, rect: Rect, panel: &RightSidebarPanelModel) {
    ui.painter().rect_filled(rect, 0, theme::PANEL_BG);
    let RightSidebarPanelModel::ImageToMidi(model) = panel;
    let total = content_height(model);
    clear_rects(ui.ctx());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 6.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = true;
    }
    egui::ScrollArea::vertical()
        .id_salt("lumino.right-sidebar.i2m")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (_, content) = ui.allocate_space(Vec2::new(
                ui.available_width(),
                total.max(ui.available_height()),
            ));
            if content.width() <= PAD * 2.0 {
                return;
            }
            draw_panel(ui, content, model);
        });

    // 内容区整块占用：点击 / 滚轮只给 egui。拖拽手柄（内容区左侧 6px）不在
    // 本矩形内，iced 仍能收到缩放拖拽。
    super::menu::extend_blocked(ui.ctx(), [rect]);
}

fn draw_panel(ui: &mut egui::Ui, rect: Rect, model: &I2mPanelModel) {
    let left = rect.left() + PAD;
    let right = rect.right() - PAD;
    let width = (right - left).max(0.0);
    let mut y = rect.top() + PAD;

    // 标题。
    ui.painter().text(
        Pos2::new(left, y + HEADER_H / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(theme::font::LABEL),
        theme::TEXT_ON_WEAK,
    );
    y += HEADER_H + GAP;

    // 选择图片按钮 + 已选提示。
    let select_w = text_width(ui, &model.select_image, theme::font::BODY) + 14.0;
    let select = Rect::from_min_size(Pos2::new(left, y), Vec2::new(select_w, BUTTON_H));
    button(
        ui,
        select,
        &model.select_image,
        ButtonStyle {
            fill: theme::TOOLBAR_BG,
            hover: theme::BASE_BG,
            text: theme::TEXT,
            enabled: true,
        },
    )
    .clicked()
    .then(|| push_action(ui.ctx(), RightSidebarAction::SelectImageFile));
    if let Some(hint) = &model.selected_hint {
        ui.painter().text(
            Pos2::new(select.right() + 6.0, select.center().y),
            Align2::LEFT_CENTER,
            hint,
            FontId::proportional(12.0),
            theme::SELECTED_BG,
        );
    }
    y += BUTTON_H + GAP;

    // 配置小节标题。
    ui.painter().text(
        Pos2::new(left, y + SECTION_H / 2.0),
        Align2::LEFT_CENTER,
        &model.config_title,
        FontId::proportional(12.0),
        theme::TEXT_ON_WEAK,
    );
    y += SECTION_H + ROW_GAP;

    for row in &model.rows {
        draw_row(ui, left, right, y, row);
        y += ROW_H + ROW_GAP;
    }

    if let Some(convert) = &model.convert {
        y += ROW_GAP;
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, BUTTON_H));
        if button(
            ui,
            rect,
            &convert.label,
            ButtonStyle {
                fill: theme::TOOLBAR_BG,
                hover: theme::ACCENT_STRONG,
                text: if convert.enabled {
                    theme::TEXT
                } else {
                    theme::TEXT_MUTED
                },
                enabled: convert.enabled,
            },
        )
        .clicked()
        {
            push_action(ui.ctx(), RightSidebarAction::ConvertClicked);
        }
    }
}

/// 一行「标签 + 右侧控件」；控件右对齐。
fn draw_row(ui: &mut egui::Ui, left: f32, right: f32, y: f32, row: &I2mRowModel) {
    let center = y + ROW_H / 2.0;
    ui.painter().text(
        Pos2::new(left, center),
        Align2::LEFT_CENTER,
        &row.label,
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );

    match &row.control {
        I2mControlModel::Input { field, text } => {
            let rect =
                Rect::from_min_size(Pos2::new(right - INPUT_W, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, rect, *field, text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *field,
                        text: next,
                    },
                );
            }
        }
        I2mControlModel::KeyRange {
            start_field,
            start_text,
            end_field,
            end_text,
        } => {
            let tilde_w = text_width(ui, "~", 12.0);
            let total = INPUT_W * 2.0 + tilde_w + 8.0;
            let mut x = right - total;
            let start = Rect::from_min_size(Pos2::new(x, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, start, *start_field, start_text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *start_field,
                        text: next,
                    },
                );
            }
            x += INPUT_W + 4.0;
            ui.painter().text(
                Pos2::new(x, center),
                Align2::LEFT_CENTER,
                "~",
                FontId::proportional(12.0),
                theme::TEXT_MUTED,
            );
            x += tilde_w + 4.0;
            let end = Rect::from_min_size(Pos2::new(x, y), Vec2::new(INPUT_W, ROW_H));
            if let Some(next) = input(ui, end, *end_field, end_text) {
                push_action(
                    ui.ctx(),
                    RightSidebarAction::I2mConfigTextChanged {
                        field: *end_field,
                        text: next,
                    },
                );
            }
        }
        I2mControlModel::Palette { options, selected } => {
            let rect =
                Rect::from_min_size(Pos2::new(right - COMBO_W, y), Vec2::new(COMBO_W, ROW_H));
            let current = options.get(*selected).cloned().unwrap_or_default();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            if let Some(index) = combo(&mut child, options, &current) {
                push_action(ui.ctx(), RightSidebarAction::I2mPaletteChanged(index));
            }
        }
    }
}

/// 数字输入框；返回变更后的文本。
fn input(ui: &mut egui::Ui, rect: Rect, field: I2mField, text: &str) -> Option<String> {
    record_rect(ui.ctx(), format!("field:{}", field as u8), rect);
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::TOOLBAR_BG,
        Stroke::new(1.0, theme::SELECTED_BG),
        egui::StrokeKind::Inside,
    );
    let mut value = text.to_owned();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("i2m-input", field as u8), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut value)
                    .frame(egui::Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(12.0))
                    .text_color(theme::TEXT)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner;
    response.changed().then_some(value)
}

/// 紧凑下拉（12px 文本、22 高、118 宽、`background.weak` 底）。
fn combo(ui: &mut egui::Ui, options: &[String], selected: &str) -> Option<usize> {
    let mut chosen = None;
    let previous_padding = ui.spacing().button_padding;
    let previous_interact = ui.spacing().interact_size;
    ui.spacing_mut().button_padding = Vec2::new(6.0, 3.0);
    ui.spacing_mut().interact_size.y = ROW_H;
    let previous_visuals = ui.visuals().clone();
    ui.visuals_mut().widgets.inactive.weak_bg_fill = theme::TOOLBAR_BG;
    ui.visuals_mut().widgets.inactive.bg_fill = theme::TOOLBAR_BG;
    ui.visuals_mut().widgets.inactive.fg_stroke.color = theme::TEXT;
    let response = egui::ComboBox::from_id_salt("lumino.right-sidebar.i2m.palette")
        .selected_text(RichText::new(selected).size(12.0).color(theme::TEXT))
        .width(COMBO_W)
        .show_ui(ui, |ui| {
            for (index, option) in options.iter().enumerate() {
                if ui
                    .selectable_label(
                        option == selected,
                        RichText::new(option.as_str()).size(12.0),
                    )
                    .clicked()
                {
                    chosen = Some(index);
                }
            }
        });
    ui.spacing_mut().button_padding = previous_padding;
    ui.spacing_mut().interact_size = previous_interact;
    *ui.visuals_mut() = previous_visuals;
    ui.painter().rect_stroke(
        response.response.rect,
        CornerRadius::same(4),
        Stroke::new(1.0, theme::SELECTED_BG),
        egui::StrokeKind::Inside,
    );
    chosen
}

/// 按钮样式参数。
struct ButtonStyle {
    fill: Color32,
    hover: Color32,
    text: Color32,
    enabled: bool,
}

/// 标准按钮：`padding 6`、圆角 4、悬停换底色。
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, style: ButtonStyle) -> egui::Response {
    record_rect(ui.ctx(), format!("button:{label}"), rect);
    let response = ui.interact(
        rect,
        Id::unique(("i2m-button", label)),
        if style.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if style.enabled && response.hovered() {
        style.hover
    } else {
        style.fill
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().text(
        Pos2::new(rect.center().x, rect.center().y),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(theme::font::BODY),
        style.text,
    );
    response
}

/// 文本宽度（与生产 eggui 同口径：比例字体度量）。
fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::TEXT)
        .size()
        .x
}
