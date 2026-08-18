//! 音频/视频导出面板与瀑布流占位页面

use iced_core::Length;
use iced_widget::{column, container, scrollable, text};

use crate::root::Root;
use crate::view::audio_export_dialog::view_audio_export_dialog;
use crate::view::video_export_dialog::view_video_export_dialog;
use crate::{Element, Theme};

impl Root {
    /// 渲染音频渲染面板（在主界面钢琴卷帘区域显示）
    pub(crate) fn view_audio_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_audio_export_panel");

        let theme = &self.window.theme;
        let palette = theme.extended_palette();

        container(
            container(scrollable(view_audio_export_dialog(
                &self.state.audio_export_dialog,
                theme,
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced_core::Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.base.color)),
                ..Default::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染视频渲染面板（在主界面钢琴卷帘区域显示）
    /// 导出进度+预览已移至独立 VideoExport 对话框窗口
    pub(crate) fn view_video_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_video_export_panel");

        let theme = &self.window.theme;
        let palette = theme.extended_palette();

        container(
            container(scrollable(view_video_export_dialog(
                &self.state.video_export_dialog,
                theme,
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced_core::Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.base.color)),
                ..Default::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染瀑布流模式占位页面（功能实现中）
    pub(crate) fn view_waterfall_placeholder(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_waterfall_placeholder");

        container(
            column![
                text("瀑布流模式")
                    .size(32)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    }),
                text("🚧 功能实现中...")
                    .size(18)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.strong.text),
                    }),
            ]
            .spacing(16)
            .align_x(iced_core::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into()
    }
}
