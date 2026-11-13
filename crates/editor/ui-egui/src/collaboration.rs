//! 协作对话框（`DialogType::Collaboration`）。
//!
//! 四种视图状态（连接 / 连接中 / 房间操作 / 房间内）。整窗 base 底、内边距 24；
//! 标题行 = 20px「多人协作」+ 弹性 + 「关闭」（12px、`padding([6,16])`），
//! 其下 20 再放当前视图内容（列居中）。
//!
//! 输入框 34 高（`base` 底 + 1px `strong` 边框、圆角 4、内边距 8 或 [8,9]），
//! 密码框掩码；按钮高度按 `padding` + 14/12px 文本推导。

use egui::{Align, Align2, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use crate::chrome::{CollaborationModel, CollaborationViewModel};
use crate::theme;

const ACTION_ID: &str = "lumino.collaboration.action";
/// 内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 输入框高度（生产 `text_input.padding(8)` + 14px 文本 + 1px 边框）。
const INPUT_H: f32 = 36.0;
/// 行间距。
const GAP_12: f32 = 13.0;
const GAP_16: f32 = 17.0;
const GAP_8: f32 = 8.0;
/// 端口输入框宽度（生产 `Length::Fixed(80)`）。
const PORT_W: f32 = 80.0;

/// 协作对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollaborationAction {
    /// 服务器地址变化。
    HostChanged(String),
    /// 端口变化。
    PortChanged(String),
    /// 用户名变化。
    UsernameChanged(String),
    /// 密码变化。
    PasswordChanged(String),
    /// 邀请码变化（连接视图与房间操作视图共用）。
    InviteCodeChanged(String),
    /// 房间名称变化。
    RoomNameChanged(String),
    /// 连接（端口解析失败时生产侧用默认端口）。
    Connect(String),
    /// 创建房间。
    CreateRoom(String),
    /// 加入房间。
    JoinRoom(String),
    /// 复制邀请码。
    CopyInviteCode,
    /// 断开连接。
    Disconnect,
    /// 关闭对话框。
    CloseDialog,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: CollaborationAction) {
    if ctx
        .data(|d| d.get_temp::<Option<CollaborationAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<CollaborationAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<CollaborationAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &CollaborationModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
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

    // 标题行。
    label(
        ui,
        egui::Rect::from_min_size(inner.min, Vec2::new(inner.width(), 27.0)),
        &model.title,
        20.0,
        theme::text(),
        Align::Min,
    );
    let close_w = text_width(ui, &model.close, 12.0) + 32.0;
    if small_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.right() - close_w, inner.top() + 1.0),
            Vec2::new(close_w, 27.0),
        ),
        &model.close,
        12.0,
        theme::toolbar_bg(),
        theme::text(),
    )
    .clicked()
    {
        push_action(ui.ctx(), CollaborationAction::CloseDialog);
    }

    let top = inner.top() + 27.0 + 20.0;
    let area = egui::Rect::from_min_max(
        egui::pos2(inner.left(), top),
        egui::pos2(inner.right(), inner.bottom()),
    );
    match &model.view {
        CollaborationViewModel::Connect(v) => connect(ui, area, v),
        CollaborationViewModel::Connecting(v) => connecting(ui, area, v),
        CollaborationViewModel::RoomActions(v) => room_actions(ui, area, v),
        CollaborationViewModel::InRoom(v) => in_room(ui, area, v),
    }
}

/// 连接视图。
fn connect(ui: &mut egui::Ui, area: egui::Rect, view: &crate::chrome::CollaborationConnectView) {
    let ctx = ui.ctx().clone();
    let mut y = area.top();
    let full = area.width();

    // 第一行：地址（填满剩余）+ 8 + 端口（80）。
    input(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(area.left(), y),
            Vec2::new(full - PORT_W - GAP_8, INPUT_H),
        ),
        "collab-host",
        &view.host,
        &view.host_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::HostChanged(text)),
    );
    input(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(area.left() + full - PORT_W, y),
            Vec2::new(PORT_W, INPUT_H),
        ),
        "collab-port",
        &view.port,
        &view.port_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::PortChanged(text)),
    );
    y += INPUT_H + GAP_12;

    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, INPUT_H)),
        "collab-username",
        &view.username,
        &view.username_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::UsernameChanged(text)),
    );
    y += INPUT_H + GAP_12;

    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, INPUT_H)),
        "collab-password",
        &view.password,
        &view.password_hint,
        true,
        |text| push_action(&ctx, CollaborationAction::PasswordChanged(text)),
    );
    y += INPUT_H + GAP_12;

    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, INPUT_H)),
        "collab-invite",
        &view.invite,
        &view.invite_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::InviteCodeChanged(text)),
    );
    y += INPUT_H + GAP_16;

    // 连接按钮：主色、宽度 = 文本 + 48（padding [8,24]）、水平居中。
    let w = text_width(ui, &view.connect, 14.0) + 48.0;
    let rect =
        egui::Rect::from_min_size(egui::pos2(area.center().x - w / 2.0, y), Vec2::new(w, 34.0));
    let port = view.port.clone();
    if primary_button(ui, rect, &view.connect, 14.0).clicked() {
        push_action(&ctx, CollaborationAction::Connect(port));
    }
    y += 34.0 + GAP_8;

    if !view.status.is_empty() {
        label(
            ui,
            egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(area.width(), 18.0)),
            &view.status,
            12.0,
            theme::text(),
            Align::Min,
        );
    }
}

/// 连接中视图（16px 主色状态 + 16 + 14px 提示，居中）。
fn connecting(
    ui: &mut egui::Ui,
    area: egui::Rect,
    view: &crate::chrome::CollaborationConnectingView,
) {
    label(
        ui,
        egui::Rect::from_min_size(area.min, Vec2::new(area.width(), 24.0)),
        &view.status,
        16.0,
        theme::accent(),
        Align::Center,
    );
    label(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(area.left(), area.top() + 24.0 + GAP_8 * 2.0),
            Vec2::new(area.width(), 20.0),
        ),
        &view.hint,
        14.0,
        theme::text(),
        Align::Center,
    );
}

/// 房间操作视图。
fn room_actions(
    ui: &mut egui::Ui,
    area: egui::Rect,
    view: &crate::chrome::CollaborationRoomActionsView,
) {
    let ctx = ui.ctx().clone();
    let full = area.width();
    let mut y = area.top();

    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, INPUT_H)),
        "collab-room",
        &view.room_name,
        &view.room_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::RoomNameChanged(text)),
    );
    y += INPUT_H + GAP_8;

    let room = view.room_name.clone();
    if primary_button(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, 34.0)),
        &view.create,
        14.0,
    )
    .clicked()
    {
        push_action(&ctx, CollaborationAction::CreateRoom(room));
    }
    y += 34.0 + GAP_16;

    label(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, 18.0)),
        &view.or,
        12.0,
        theme::text(),
        Align::Center,
    );
    y += 18.0 + GAP_16;

    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, INPUT_H)),
        "collab-join-invite",
        &view.invite,
        &view.invite_hint,
        false,
        |text| push_action(&ctx, CollaborationAction::InviteCodeChanged(text)),
    );
    y += INPUT_H + GAP_8;

    let invite = view.invite.clone();
    if small_button(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, 34.0)),
        &view.join,
        14.0,
        theme::toolbar_bg(),
        theme::text(),
    )
    .clicked()
    {
        push_action(&ctx, CollaborationAction::JoinRoom(invite));
    }
}

/// 房间内视图。
fn in_room(ui: &mut egui::Ui, area: egui::Rect, view: &crate::chrome::CollaborationInRoomView) {
    let ctx = ui.ctx().clone();
    let full = area.width();
    let mut y = area.top();

    label(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.left(), y), Vec2::new(full, 22.0)),
        &view.room_line,
        16.0,
        theme::text(),
        Align::Center,
    );
    y += 22.0 + GAP_8;

    // 「邀请码: {code}」：整行居中，码用主色。
    let label_w = text_width(ui, &view.invite_label, 14.0);
    let code_w = text_width(ui, &view.invite_code, 14.0);
    let start = area.center().x - (label_w + code_w) / 2.0;
    ui.painter().text(
        egui::pos2(start, y + 10.0),
        Align2::LEFT_CENTER,
        &view.invite_label,
        FontId::proportional(14.0),
        theme::text(),
    );
    ui.painter().text(
        egui::pos2(start + label_w, y + 10.0),
        Align2::LEFT_CENTER,
        &view.invite_code,
        FontId::proportional(14.0),
        theme::accent(),
    );
    y += 20.0 + GAP_16;

    let copy_w = text_width(ui, &view.copy, 12.0) + 32.0;
    if small_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(area.center().x - copy_w / 2.0, y),
            Vec2::new(copy_w, 27.0),
        ),
        &view.copy,
        12.0,
        theme::toolbar_bg(),
        theme::text(),
    )
    .clicked()
    {
        push_action(&ctx, CollaborationAction::CopyInviteCode);
    }
    y += 27.0 + 24.0;

    let w = text_width(ui, &view.disconnect, 14.0) + 48.0;
    if small_button(
        ui,
        egui::Rect::from_min_size(egui::pos2(area.center().x - w / 2.0, y), Vec2::new(w, 34.0)),
        &view.disconnect,
        14.0,
        theme::danger(),
        Color32::WHITE,
    )
    .clicked()
    {
        push_action(&ctx, CollaborationAction::Disconnect);
    }
}

/// 固定盒子输入框（`base` 底 + 1px `strong` 边框、圆角 4）。
fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    value: &str,
    placeholder: &str,
    password: bool,
    changed: impl FnOnce(String),
) {
    let mut buffer = value.to_owned();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(9.0, 8.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("collab-input", id), |ui| {
            let mut edit = egui::TextEdit::singleline(&mut buffer)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(FontId::proportional(theme::font::LABEL))
                .text_color(theme::text())
                .hint_text(
                    RichText::new(placeholder)
                        .size(theme::font::LABEL)
                        .color(theme::text_dim()),
                )
                .desired_width(rect.width() - 20.0);
            if password {
                edit = edit.password(true);
            }
            ui.add(edit)
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 主色按钮。
fn primary_button(ui: &mut egui::Ui, rect: egui::Rect, label: &str, size: f32) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(size).color(theme::on_accent()))
        .fill(theme::accent())
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 次要按钮（底色可指定）。
fn small_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(size).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
        .ceil()
}

/// 固定矩形内的文本标签（左 / 居中），保留可访问树节点。
fn label(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32, align: Align) {
    let layout = match align {
        Align::Center => egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        _ => egui::Layout::left_to_right(Align::Center),
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
