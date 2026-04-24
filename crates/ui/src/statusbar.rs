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
    /// FPS 值（仅 macOS 调试模式使用，显示在底部状态栏代替"就绪"）
    fps: Option<f32>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
            fps: None,
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

    /// 设置 FPS 值（仅 macOS 使用）
    pub fn set_fps(&mut self, fps: f32) {
        self.fps = Some(fps);
    }

    pub fn view<'a>(&'a self) -> Element<'a> {
        let (left_text, use_fps_style) = if self.info.left_text.is_empty() {
            if cfg!(target_os = "macos") {
                // macOS: 在状态栏显示 FPS 代替默认的"就绪"
                if let Some(fps) = self.fps {
                    (format!("FPS: {:.1}", fps), true)
                } else {
                    ("就绪".to_string(), false)
                }
            } else {
                ("就绪".to_string(), false)
            }
        } else {
            (self.info.left_text.clone(), false)
        };

        container(
            row![
                text(left_text).size(12).style(move |theme: &Theme| {
                    if use_fps_style {
                        let palette = theme.extended_palette();
                        text::Style {
                            color: Some(palette.primary.strong.color),
                        }
                    } else {
                        text::Style::default()
                    }
                }),
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
