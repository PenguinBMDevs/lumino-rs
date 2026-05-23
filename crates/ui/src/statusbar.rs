pub mod performance;

use iced_core::Length;
use iced_widget::{button, container, row, text};

use super::Element;
use crate::Theme;
use performance::PerfData;

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
    /// FPS 值（显示在底部状态栏代替"就绪"）
    fps: Option<f32>,
    /// 性能监控数据
    perf_data: PerfData,
    /// 性能面板是否展开
    pub perf_panel_expanded: bool,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
            fps: None,
            perf_data: PerfData::default(),
            perf_panel_expanded: false,
        }
    }

    /// 设置 FPS 值
    pub fn set_fps(&mut self, fps: f32) {
        self.fps = Some(fps);
    }

    /// 设置性能监控数据
    pub fn set_perf_data(&mut self, data: PerfData) {
        self.perf_data = data;
    }

    /// 切换性能面板展开/折叠
    pub fn toggle_perf_panel(&mut self) {
        self.perf_panel_expanded = !self.perf_panel_expanded;
    }

    /// 获取性能数据引用
    pub fn perf_data(&self) -> &PerfData {
        &self.perf_data
    }

    /// 获取 FPS 文本内容
    pub fn fps_text(&self) -> String {
        if self.info.left_text.is_empty() {
            if let Some(fps) = self.fps {
                format!("FPS: {:.1}", fps)
            } else {
                "就绪".to_string()
            }
        } else {
            self.info.left_text.clone()
        }
    }

    /// 是否使用 FPS 样式
    pub fn use_fps_style(&self) -> bool {
        self.info.left_text.is_empty() && self.fps.is_some()
    }

    pub fn view<'a>(&'a self) -> Element<'a> {
        let left_text = self.fps_text();
        let use_fps_style = self.use_fps_style();

        let arrow = if self.perf_panel_expanded {
            " ▼"
        } else {
            " ▲"
        };

        let left_section: Element<'a> = row![
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
            button(text(arrow).size(10))
                .on_press(crate::Message::PerformancePanelToggled)
                .padding([0, 2])
                .style(|_: &Theme, _: button::Status| button::Style::default()),
        ]
        .into();

        container(
            row![
                left_section,
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
