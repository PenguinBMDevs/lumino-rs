//! 设置面板 — 云存储管理页
//!
//! 连接列表（在线/离线状态标志）、添加连接、连接/断开/删除、
//! 打开文件管理面板、断连提醒标志显示。

use iced_core::{Alignment, Length};
use iced_widget::{Space, button, column, container, row, scrollable, text};

use lumino_ui_core::{Element, Message, Theme};

use crate::{CloudConnItem, SettingsPanel};
use lumino_message::CloudAction;

/// 渲染云存储管理页
pub fn view(settings: &SettingsPanel) -> Element<'static> {
    // 断连提醒标志（Q5：失败提醒在设置面板始终可见，实时更新）
    let alert: Element<'static> = settings
        .cloud_alert
        .as_deref()
        .map(|msg| {
            container(
                text(format!("⚠ 云存储提醒：{msg}"))
                    .size(12)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().danger.base.color),
                    }),
            )
            .padding(8)
            .width(Length::Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(
                    theme.extended_palette().danger.weak.color,
                )),
                border: iced_core::Border::default().rounded(4),
                ..Default::default()
            })
            .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());

    // 添加连接按钮
    let add_btn = button(text("添加连接").size(13))
        .padding([6, 18])
        .style(|theme: &Theme, _status| button::Style {
            background: Some(iced_core::Background::Color(
                theme.extended_palette().primary.base.color,
            )),
            text_color: theme.extended_palette().primary.strong.text,
            border: iced_core::Border::default().rounded(6),
            ..Default::default()
        })
        .on_press(Message::Cloud(CloudAction::OpenConnectPanel));

    // 连接列表
    let list: Element<'static> = if settings.cloud_connections.is_empty() {
        text("尚未添加云存储连接。支持 FTP / SFTP / WebDAV，连接信息仅存储在本机。")
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            })
            .into()
    } else {
        scrollable(
            column(
                settings
                    .cloud_connections
                    .iter()
                    .map(conn_row)
                    .collect::<Vec<_>>(),
            )
            .spacing(6),
        )
        .height(Length::Fill)
        .into()
    };

    container(
        column![
            text("云存储管理").size(16),
            alert,
            row![add_btn, Space::new().width(Length::Fill)].align_y(Alignment::Center),
            list,
        ]
        .spacing(12),
    )
    .padding(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 单个连接行：名称 [协议] 地址 · 状态标志 · 操作按钮
fn conn_row(conn: &CloudConnItem) -> Element<'static> {
    let status = if conn.online {
        "🟢 在线"
    } else {
        "🔴 离线"
    };

    let connect_btn = if conn.online {
        button(text("断开").size(12))
            .padding([3, 10])
            .on_press(Message::Cloud(CloudAction::Disconnect(conn.id.clone())))
    } else {
        button(text("连接").size(12))
            .padding([3, 10])
            .on_press(Message::Cloud(CloudAction::ConnectExisting(
                conn.id.clone(),
            )))
    };

    let manage_btn = button(text("管理文件").size(12))
        .padding([3, 10])
        .on_press_maybe(
            conn.online
                .then_some(Message::Cloud(CloudAction::OpenBrowserPanel)),
        );

    let delete_btn = button(text("删除").size(12))
        .padding([3, 10])
        .on_press(Message::Cloud(CloudAction::DeleteConnection(
            conn.id.clone(),
        )));

    row![
        container(
            column![
                text(format!("{} [{}]", conn.name, conn.protocol)).size(13),
                text(conn.address.clone())
                    .size(11)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.strong.text),
                    }),
            ]
            .spacing(2)
            .align_x(Alignment::Start),
        )
        .width(Length::Fill),
        text(status).size(12),
        connect_btn,
        manage_btn,
        delete_btn,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(6)
    .into()
}
