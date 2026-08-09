//! 云存储连接面板渲染
//!
//! 协议/名称/地址/端口/用户名/密码 + 隐私提示 + 错误回显。
//! 文件浏览面板见 `cloud_browser` 模块。

use iced_core::{Alignment, Length};
use iced_widget::{Space, button, column, container, pick_list, row, text, text_input};

use lumino_message::{CloudAction, CloudProtocolUi};

use crate::root::Root;
use crate::state::cloud_state::CloudUiState;
use crate::state::root_state::DialogType;
use crate::{Element, Message, Theme};

/// 隐私提示（需求 8：明示本地存储）
const PRIVACY_HINT: &str = "Lumino 无权且不会收集您的个人信息，所有信息将会存储在本地";

/// 渲染云存储对话框（按 dialog_type 区分连接面板与文件浏览面板）
pub fn view_cloud_dialog<'a>(root: &'a Root, theme: &Theme) -> Element<'a> {
    match root.state.dialog_type {
        DialogType::CloudConnect => view_connect_panel(&root.cloud, theme),
        _ => crate::view::cloud_browser::view_cloud_browser(&root.cloud, theme),
    }
}

/// 云连接面板
fn view_connect_panel<'a>(state: &'a CloudUiState, _theme: &Theme) -> Element<'a> {
    let protocols: &[CloudProtocolUi] = &[
        CloudProtocolUi::Ftp,
        CloudProtocolUi::Sftp,
        CloudProtocolUi::Webdav,
    ];

    let form = column![
        label_row(
            "协议",
            pick_list(protocols, Some(state.protocol), |p| {
                Message::Cloud(CloudAction::ProtocolSelected(p))
            })
        ),
        label_row(
            "名称（可选）",
            text_input("例如：我的 NAS", &state.name)
                .on_input(|s| Message::Cloud(CloudAction::NameChanged(s)))
        ),
        label_row(
            "服务器地址",
            text_input("host.example.com", &state.address)
                .on_input(|s| Message::Cloud(CloudAction::AddressChanged(s)))
        ),
        label_row(
            "端口（可选）",
            text_input("留空使用默认端口", &state.port)
                .on_input(|s| Message::Cloud(CloudAction::PortChanged(s)))
        ),
        label_row(
            "用户名",
            text_input("用户名", &state.username)
                .on_input(|s| Message::Cloud(CloudAction::UsernameChanged(s)))
        ),
        label_row(
            "密码（可选）",
            text_input("密码", &state.password)
                .on_input(|s| Message::Cloud(CloudAction::PasswordChanged(s)))
        ),
    ]
    .spacing(10);

    // 错误提示（连接失败原因）
    let error_hint: Element<'static> = state
        .connect_error
        .as_deref()
        .map(|e| {
            text(format!("连接失败：{e}"))
                .size(13)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().danger.base.color),
                })
                .into()
        })
        .unwrap_or_else(|| Space::new().height(4).into());

    let connect_btn = button(
        text(if state.connecting {
            "连接中..."
        } else {
            "连接"
        })
        .size(14),
    )
    .padding([8, 28])
    .style(|theme: &Theme, status| {
        let palette = theme.extended_palette();
        let bg = if state.connecting {
            palette.background.weak.color
        } else {
            palette.primary.base.color
        };
        button::Style {
            background: Some(iced_core::Background::Color(bg)),
            text_color: palette.primary.strong.text,
            border: iced_core::Border::default().rounded(6),
            ..Default::default()
        }
        .with_background(if matches!(status, button::Status::Hovered) {
            palette.primary.strong.color
        } else {
            bg
        })
    })
    .on_press(Message::Cloud(CloudAction::Connect));

    let cancel_btn = button(text("取消").size(14))
        .padding([8, 20])
        .style(|theme: &Theme, _status| button::Style {
            background: Some(iced_core::Background::Color(
                theme.extended_palette().background.weak.color,
            )),
            ..Default::default()
        })
        .on_press(Message::Cloud(CloudAction::ConnectCancel));

    container(
        column![
            text("连接云存储").size(18),
            form,
            error_hint,
            text(PRIVACY_HINT)
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
            row![cancel_btn, connect_btn]
                .spacing(12)
                .align_y(Alignment::Center),
        ]
        .spacing(14)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(24)
    .style(|theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(theme.palette().background)),
        ..Default::default()
    })
    .into()
}

/// 标签 + 控件行
fn label_row<'a>(label: &'a str, widget: impl Into<Element<'a>>) -> Element<'a> {
    row![
        text(label).size(13).width(Length::Fixed(110.0)),
        container(widget.into()).width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_cloud_dialog_returns_element() {
        // 仅验证渲染函数可调用（不 panic）
        let root = crate::root::Root::new_dialog("dark", DialogType::CloudConnect);
        let _el = view_cloud_dialog(&root, &crate::Theme::Dark);
    }
}
