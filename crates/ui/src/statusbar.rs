pub mod performance;

use iced_core::Length;
use iced_widget::{button, container, row, space, text};
use lumino_extras::i18n::{Language, main_translations};
use lumino_ui_core::button_descs::ButtonId;

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
    /// 由 `set_hover_label` 写入按钮角色标识：鼠标悬停时显示
    /// `按钮名 - {解释说明}`，离开时清空。
    hover_label: Option<ButtonId>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
            fps: None,
            perf_data: PerfData::default(),
            hover_label: None,
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

    /// 设置/清除悬停描述文字。
    ///
    /// `Some(id)` 表示鼠标正悬停在标识为 `id` 的工具栏按钮上；
    /// `None` 表示鼠标已离开，清空描述区。
    pub fn set_hover_label(&mut self, label: Option<ButtonId>) {
        self.hover_label = label;
    }

    /// 是否处于 FPS 高亮样式（仅当左侧无描述文字且存在 FPS 值时）
    pub fn use_fps_style(&self) -> bool {
        self.hover_label.is_none() && self.info.left_text.is_empty() && self.fps.is_some()
    }

    pub fn view<'a>(&'a self, language: Language) -> Element<'a> {
        let translations = main_translations(language);

        // 左侧描述区：优先显示悬停按钮的 `按钮名 - {解释说明}`，
        // 其次为显式 left_text，最后为"就绪"
        let left_text: String = if let Some(id) = self.hover_label {
            let (name, desc) = lumino_ui_core::button_descs::button_desc(id, language);
            format!("{} - {}", name, desc)
        } else if !self.info.left_text.is_empty() {
            self.info.left_text.clone()
        } else {
            translations.status_ready.to_string()
        };
        let use_fps_style = self.use_fps_style();

        // 左侧预留描述区（固定宽度，避免 hover 时布局跳动）
        // 说明文字强制单行显示：不设置 width=Fill，文本默认 Shrink 不换行，
        // 超出 220px 容器宽度的内容被裁剪，绝不发生折行。
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

        // 右侧性能指标组：CPU / MEM / FPS（仿照 yinhe mode_bar 底部栏的 metric 设计：
        // label 用弱色、value 用强调色；MEM 可点击打开内存监控对话框）
        let perf_section: Element<'a> = if let Some(fps) = self.fps {
            row![
                metric_label("CPU"),
                metric_value(format!("{:.1}%", self.perf_data.cpu_usage)),
                space().width(12),
                metric_label("MEM"),
                metric_clickable_value(
                    format!("{:.1} MB", self.perf_data.memory_mb),
                    crate::toolbar::Event::open_memory_monitor_dialog(),
                ),
                space().width(12),
                metric_label("FPS"),
                metric_value(format!("{:.1}", fps)),
            ]
            .align_y(iced_core::Alignment::Center)
            .into()
        } else {
            iced_widget::Space::new().into()
        };

        container(
            row![
                left_section,
                iced_widget::space().width(Length::Fill),
                perf_section,
                text(&self.info.right_text).size(12),
            ]
            .spacing(8)
            .padding([0, 8])
            .align_y(iced_core::Alignment::Center),
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

/// 指标标签（弱色小字，仿照 yinhe mode_bar 的 `metric` 标签）
fn metric_label<'a>(label: &'a str) -> Element<'a> {
    text(label)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.background.weak.text),
            }
        })
        .into()
}

/// 指标数值（强调色，仿照 yinhe mode_bar 的 `metric` 数值）
fn metric_value<'a>(value: String) -> Element<'a> {
    text(value)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.primary.strong.color),
            }
        })
        .into()
}

/// 可点击的指标数值（强调色，点击发送消息，仿照 yinhe `metric_clickable`）
fn metric_clickable_value<'a>(value: String, on_press: crate::Message) -> Element<'a> {
    button(text(value).size(12).style(|theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.primary.strong.color),
        }
    }))
    .on_press(on_press)
    .padding([0.0, 0.0])
    .style(|theme: &Theme, _status| {
        let palette = theme.extended_palette();
        button::Style {
            background: None,
            text_color: palette.primary.strong.color,
            border: iced_core::Border::default(),
            shadow: Default::default(),
            snap: false,
        }
    })
    .into()
}
