//! Help 窗口（`DialogType::Help`）。
//!
//! 左侧 240 宽「分区 + 话题」列表（`column.spacing(3)`，分区 11px、话题 13px，
//! 选中 `primary.weak` 底），右侧正文（标题 22px、tip 12px、正文 14px，
//! 段落间 10px），顶部全宽搜索框，底部「全部重现」+「关闭」。
//! 整窗内边距 16、列间距 2。
//!
//! 列表与正文各自在 `ScrollArea` 内（8px 实体滚动条），与生产 `scrollable`
//! 一致；搜索框只回传原样文本，过滤仍由生产 `help::search` 负责。

use egui::{
    Align, Color32, CornerRadius, FontId, Id, Layout, Rect, RichText, ScrollArea, Stroke, Vec2,
};

use crate::chrome::HelpModel;
use crate::theme;

const ACTION_ID: &str = "lumino.help.action";
/// 整窗内边距（生产 `container(...).padding(16)`）。
const PAD: f32 = 16.0;
/// 标题行高（20px 文本，含 iced 1.5 倍行高）。
const TITLE_H: f32 = 30.0;
/// 搜索框（生产 `text_input.padding([7,10])`、全宽）。
const SEARCH_TOP: f32 = 40.0;
const SEARCH_H: f32 = 40.0;
const SEARCH_GAP: f32 = 12.0;
/// 左侧话题列表宽度（生产 `Length::Fixed(240)`）与列间距。
const LIST_W: f32 = 240.0;
const COLUMN_GAP: f32 = 18.0;
/// 话题行（生产 `button.padding([6,10])`、`spacing(3)`）。
const ROW_H: f32 = 28.0;
const ROW_GAP: f32 = 3.0;
/// 底部按钮（生产 `padding([6,14])` / `[6,16]`）。
const BUTTONS_H: f32 = 28.0;
const BUTTONS_GAP: f32 = 10.0;
/// 正文内边距（生产 `scrollable(content).padding([4,12])`）。
const BODY_PAD_V: f32 = 4.0;
const BODY_PAD_H: f32 = 12.0;

/// Help 窗口产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpAction {
    /// 搜索框文本变化。
    QueryChanged(String),
    /// 选中过滤结果里的第 `index` 个话题。
    TopicSelected(usize),
    /// 清空「已看过」记录（全部重现）。
    ResetTips,
    /// 关闭窗口。
    Close,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: HelpAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<HelpAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<HelpAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<HelpAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &HelpModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    label_block(
        ui,
        Rect::from_min_size(inner.min, Vec2::new(inner.width(), TITLE_H)),
        &model.title,
        20.0,
        theme::text(),
    );
    search_box(ui, inner, model);

    // 列表 + 正文：从搜索框下方到按钮行上方。
    let main_top = inner.top() + SEARCH_TOP + SEARCH_H + SEARCH_GAP;
    let main_bottom = inner.bottom() - BUTTONS_H - BUTTONS_GAP;
    if main_bottom > main_top {
        let list_rect = Rect::from_min_max(
            egui::pos2(inner.left(), main_top),
            egui::pos2(inner.left() + LIST_W, main_bottom),
        );
        topic_list(ui, list_rect, model);
        let body_rect = Rect::from_min_max(
            egui::pos2(list_rect.right() + COLUMN_GAP, main_top),
            egui::pos2(inner.right(), main_bottom),
        );
        topic_body(ui, body_rect, model);
    }

    buttons_row(ui, inner, model);
}

/// 搜索框：全宽、`base` 底 + 1px `strong` 边框（圆角 4）、内边距 7/10。
fn search_box(ui: &mut egui::Ui, inner: Rect, model: &HelpModel) {
    let mut buffer = model.query.clone();
    let rect = Rect::from_min_size(
        egui::pos2(inner.left(), inner.top() + SEARCH_TOP),
        Vec2::new(inner.width(), SEARCH_H),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 7.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id("help-search", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(theme::font::LABEL))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(&model.search_hint)
                            .size(theme::font::LABEL)
                            .color(theme::text_dim()),
                    )
                    .desired_width(rect.width() - 22.0),
            )
        })
        .inner;
    if response.changed() {
        push_action(ui.ctx(), HelpAction::QueryChanged(buffer));
    }
}

/// 左侧列表：分区标题（11px）+ 话题行（13px，选中 `primary.weak` 底）。
fn topic_list(ui: &mut egui::Ui, rect: Rect, model: &HelpModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
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

    if model.sections.is_empty() {
        let mut empty = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        empty.add(egui::Label::new(
            RichText::new(&model.empty)
                .size(12.0)
                .color(theme::text_muted()),
        ));
        return;
    }

    ScrollArea::vertical()
        .id_salt("lumino.help.list")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let width = ui.available_width().max(1.0);
            for section in &model.sections {
                if !section.topics.is_empty() {
                    ui.add(egui::Label::new(
                        RichText::new(&section.title)
                            .size(11.0)
                            .color(theme::text_muted()),
                    ));
                    for topic in &section.topics {
                        let (r, response) =
                            ui.allocate_exact_size(Vec2::new(width, ROW_H), egui::Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &topic.label)
                        });
                        if topic.active {
                            ui.painter().rect_filled(
                                r,
                                CornerRadius::same(4),
                                theme::accent_dark(),
                            );
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                r,
                                CornerRadius::same(4),
                                theme::selected_bg(),
                            );
                        }
                        let color = if topic.active {
                            theme::text_on_weak()
                        } else {
                            theme::text()
                        };
                        ui.painter().text(
                            egui::pos2(r.left() + 10.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            &topic.label,
                            FontId::proportional(theme::font::BODY),
                            color,
                        );
                        if response.clicked() {
                            push_action(ui.ctx(), HelpAction::TopicSelected(topic.index));
                        }
                    }
                }
            }
        });
}

/// 右侧正文：标题 22px、tip 12px、段落 14px（段间 10px），整体可滚动。
fn topic_body(ui: &mut egui::Ui, rect: Rect, model: &HelpModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + BODY_PAD_H, rect.top() + BODY_PAD_V),
        egui::pos2(rect.right() - BODY_PAD_H, rect.bottom() - BODY_PAD_V),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
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
        .id_salt("lumino.help.body")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let Some(topic) = &model.topic else {
                ui.add(egui::Label::new(
                    RichText::new(&model.empty).size(14.0).color(theme::text()),
                ));
                return;
            };
            ui.add(egui::Label::new(
                RichText::new(&topic.title).size(22.0).color(theme::text()),
            ));
            // 间隙按 iced 的 1.5 倍行高折算：标题→tip 12，tip→正文 20，
            // 段落之间 16（egui 的行高比 iced 小，需要更大的显式间距才能对齐
            // 真实窗口里的墨迹行）。
            ui.add_space(12.0);
            ui.add(egui::Label::new(
                RichText::new(&topic.tip)
                    .size(12.0)
                    .color(theme::text_muted()),
            ));
            ui.add_space(20.0);
            for paragraph in &topic.paragraphs {
                ui.add(
                    egui::Label::new(
                        RichText::new(paragraph)
                            .size(theme::font::LABEL)
                            .color(theme::text()),
                    )
                    .wrap(),
                );
                ui.add_space(16.0);
            }
        });
}

/// 底部：「全部重现」（次要）+ 弹性 + 「关闭」（次要）。
fn buttons_row(ui: &mut egui::Ui, inner: Rect, model: &HelpModel) {
    let y = inner.bottom() - BUTTONS_H;
    // `button_width` 已含两侧 padding（生产 [6,14] / [6,16]），不要再叠加。
    let reset_width = button_width(ui, &model.reset, 14.0);
    let close_width = button_width(ui, &model.close, 16.0);

    let reset_rect = Rect::from_min_size(
        egui::pos2(inner.left(), y),
        Vec2::new(reset_width, BUTTONS_H),
    );
    if button(ui, reset_rect, &model.reset, 13.0).clicked() {
        push_action(ui.ctx(), HelpAction::ResetTips);
    }
    let close_rect = Rect::from_min_size(
        egui::pos2(inner.right() - close_width, y),
        Vec2::new(close_width, BUTTONS_H),
    );
    if button(ui, close_rect, &model.close, 13.0).clicked() {
        push_action(ui.ctx(), HelpAction::Close);
    }
}

/// 文本宽度（用于按生产 padding 反推按钮宽度）。
fn button_width(ui: &egui::Ui, label: &str, horizontal_padding: f32) -> f32 {
    let text = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), theme::text())
        .size()
        .x
        .ceil();
    text + horizontal_padding * 2.0
}

/// 底部按钮：iced 默认 `button` 样式 = 主色底 + 主色上的文字（白/深）。
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, size: f32) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(size).color(theme::on_accent()))
        .fill(theme::accent())
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
