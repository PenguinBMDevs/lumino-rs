pub mod performance;

use iced_core::Length;
use iced_widget::{container, row, text};
use lumino_core::i18n::{Language, main_translations};

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
    /// FPS 值（显示在底部状态栏右侧）
    fps: Option<f32>,
    /// 性能监控数据
    perf_data: PerfData,
    /// 悬停工具栏按钮时显示的描述文字（左侧预留区）
    ///
    /// 由 `set_hover_label` 写入：鼠标悬停按钮时显示按钮名，离开时清空。
    hover_label: String,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
            fps: None,
            perf_data: PerfData::default(),
            hover_label: String::new(),
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

    /// 获取性能数据引用
    pub fn perf_data(&self) -> &PerfData {
        &self.perf_data
    }

    /// 设置/清除悬停描述文字。
    ///
    /// `Some(name)` 表示鼠标正悬停在名为 `name` 的工具栏按钮上；
    /// `None` 表示鼠标已离开，清空描述区。
    pub fn set_hover_label(&mut self, label: Option<String>) {
        match label {
            Some(name) => self.hover_label = name,
            None => self.hover_label.clear(),
        }
    }

    /// 是否处于 FPS 高亮样式（仅当左侧无描述文字且存在 FPS 值时）
    pub fn use_fps_style(&self) -> bool {
        self.hover_label.is_empty() && self.info.left_text.is_empty() && self.fps.is_some()
    }

    pub fn view<'a>(&'a self, language: Language) -> Element<'a> {
        let t = main_translations(language);

        // 左侧描述区：优先显示悬停按钮名，其次为显式 left_text，最后为"就绪"
        let left_text: &str = if !self.hover_label.is_empty() {
            &self.hover_label
        } else if !self.info.left_text.is_empty() {
            &self.info.left_text
        } else {
            t.status_ready
        };
        let use_fps_style = self.use_fps_style();

        // 左侧预留描述区（固定最小宽度，避免 hover 时布局跳动）
        let left_section: Element<'a> =
            container(text(left_text).size(12).style(move |theme: &Theme| {
                if use_fps_style {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.primary.strong.color),
                    }
                } else {
                    text::Style::default()
                }
            }))
            .width(Length::Fixed(220.0))
            .into();

        // 右侧 FPS（移至此显示）
        let fps_section: Element<'a> = if let Some(fps) = self.fps {
            text(format!("FPS: {:.1}", fps))
                .size(12)
                .style(move |theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.primary.strong.color),
                    }
                })
                .into()
        } else {
            iced_widget::Space::new().into()
        };

        container(
            row![
                left_section,
                iced_widget::space().width(Length::Fill),
                fps_section,
                text(&self.info.right_text).size(12),
            ]
            .spacing(8)
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
