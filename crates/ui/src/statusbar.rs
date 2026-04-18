use iced_core::Length;
use iced_widget::{container, row, text};

use super::Element;
use crate::Theme;

/// 状态栏显示的信息
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    /// 左侧状态文本
    pub left_text: String,
    /// 右侧状态文本
    pub right_text: String,
}

pub struct StatusBar {
    info: StatusInfo,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
        }
    }

    /// 更新状态栏信息
    pub fn update_info(&mut self, info: StatusInfo) {
        self.info = info;
    }

    /// 设置左侧状态文本
    pub fn set_left_text(&mut self, text: impl Into<String>) {
        self.info.left_text = text.into();
    }

    /// 设置右侧状态文本
    pub fn set_right_text(&mut self, text: impl Into<String>) {
        self.info.right_text = text.into();
    }

    pub fn view<'a>(&'a self) -> Element<'a> {
        let left_text = if self.info.left_text.is_empty() {
            "就绪".to_string()
        } else {
            self.info.left_text.clone()
        };

        container(
            row![
                text(left_text).size(12),
                iced_widget::space().width(Length::Fill),
                text(&self.info.right_text).size(12),
            ]
            .padding([0, 8]),
        )
        .width(Length::Fill)
        .height(20)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weak.color)
        })
        .into()
    }
}
