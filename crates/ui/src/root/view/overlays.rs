//! 覆盖层/弹窗视图渲染函数
//!
//! 包含进度窗口和对话框窗口的渲染。

use iced_core::Length;
use iced_widget::{column, container, progress_bar, space, text};

use crate::root::{Element, Root, Theme};
use crate::state::root_state::DialogType;
use crate::view::{
    collaboration_dialog::view_collaboration_dialog,
    custom_precision_dialog::view_custom_precision_dialog,
    load_confirm_dialog::view_load_confirm_dialog,
    project_settings_dialog::view_project_settings_dialog, settings_dialog::view_settings_dialog,
    speed_change_dialog::view_speed_change_dialog,
};

impl Root {
    /// 渲染进度窗口
    pub(super) fn view_progress(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_progress");

        // 进度窗口只显示进度
        // 默认显示初始化状态，避免窗口空白
        let (msg, progress) = self
            .progress
            .as_ref()
            .map(|(m, p)| (m.as_str(), *p))
            .unwrap_or(("正在初始化...", 0.0));

        container(
            column![
                text("处理中...")
                    .size(24)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    }),
                text(msg).size(16).style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.neutral.text),
                }),
                progress_bar(0.0..=1.0, progress as f32),
            ]
            .spacing(20)
            .align_x(iced_core::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(30)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into()
    }

    /// 渲染对话框
    pub(super) fn view_dialog(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_dialog");

        // 对话框窗口 - 根据类型显示不同内容
        match self.state.dialog_type {
            DialogType::Collaboration => {
                view_collaboration_dialog(&self.state.collaboration_dialog, &self.window.theme)
            }
            DialogType::LoadConfirm => {
                view_load_confirm_dialog(&self.state.load_confirm_dialog, &self.window.theme)
            }
            DialogType::ProjectSettings => view_project_settings_dialog(
                &self.state.project_settings_dialog,
                &self.window.theme,
                self.settings.language,
            ),
            DialogType::Settings => {
                view_settings_dialog(&self.settings, &self.window, &self.state.system_fonts)
            }
            DialogType::SpeedChange => {
                view_speed_change_dialog(&self.state.speed_change_dialog, &self.window.theme)
            }
            DialogType::CustomPrecision => view_custom_precision_dialog(
                &self.state.custom_precision_dialog,
                &self.window.theme,
                self.settings.language,
            ),
            // DialogType::None: 关闭过程中 dialog_type 可能被复位为 None，
            // 此时渲染空容器避免闪跳到精度面板。实际关闭由 DialogWindow 的 should_close 驱动。
            DialogType::None => container(space())
                .width(iced_core::Length::Fill)
                .height(iced_core::Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(theme.palette().background)),
                    ..Default::default()
                })
                .into(),
        }
    }
}
