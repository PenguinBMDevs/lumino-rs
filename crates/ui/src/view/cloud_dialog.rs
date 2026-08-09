//! 云存储对话框渲染（连接面板 / 文件浏览面板）
//!
//! - `CloudConnect`：连接面板（协议/地址/用户名/密码）
//! - `CloudBrowser`：文件浏览面板（仿资源管理器）
//!
//! 当前为占位实现（Phase 3/4 填充完整 UI），保证对话框可打开并正确显示。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, text};

use crate::root::Root;
use crate::state::root_state::DialogType;
use crate::{Element, Theme};

/// 渲染云存储对话框（按 dialog_type 区分连接面板与文件浏览面板）
pub fn view_cloud_dialog<'a>(root: &'a Root, _theme: &Theme) -> Element<'a> {
    let title = match root.state.dialog_type {
        DialogType::CloudConnect => "连接云存储",
        _ => "云存储文件",
    };
    let hint = match root.state.dialog_type {
        DialogType::CloudConnect => "输入 FTP / SFTP / WebDAV 连接信息",
        _ => "云文件浏览面板",
    };

    container(
        column![
            text(title).size(18),
            text(hint).size(13),
            button(text("关闭").size(13)).on_press(crate::Message::Null),
        ]
        .spacing(16)
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
