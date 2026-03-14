//! Root 视图渲染子模块

use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, text};
use lumino_gfx::NoteInstance;

use crate::root::{Element, Root, Theme};
use crate::state::root_state::DialogType;
use crate::view::{
    collaboration_dialog::view_collaboration_dialog,
    custom_precision_dialog::view_custom_precision_dialog,
};
use crate::{message, settings, sidebar, statusbar, titlebar, toolbar};

impl Root {
    /// 渲染视图
    pub fn view(&self) -> Element<'_> {
        if self.is_progress_window {
            self.view_progress()
        } else if self.state.is_dialog_window {
            self.view_dialog()
        } else {
            self.view_main()
        }
    }

    /// 渲染进度窗口
    fn view_progress(&self) -> Element<'_> {
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
    fn view_dialog(&self) -> Element<'_> {
        // 对话框窗口 - 根据类型显示不同内容
        match self.state.dialog_type {
            DialogType::Collaboration => {
                view_collaboration_dialog(&self.state.collaboration_dialog, &self.window.theme)
            }
            _ => view_custom_precision_dialog(
                &self.state.custom_precision_dialog,
                &self.window.theme,
            ),
        }
    }

    /// 渲染主窗口
    fn view_main(&self) -> Element<'_> {
        let is_settings_route = self.sidebar.is_settings_route();

        // 左侧栏（包含图标栏和音轨面板）
        let left_bar = self.sidebar.view(&self.window);

        // 主内容区域（工具栏 + 编辑器/设置）
        let main_area: Element<'_> = if is_settings_route {
            // 设置路由激活时显示设置界面
            settings::view(&self.settings, &self.window)
        } else {
            // 默认显示工具栏 + 编辑器
            column![
                self.toolbar.view(&self.window),
                self.editor.view(
                    message::Message::ScrollbarScrolled,
                    message::Message::ScrollbarScrolledY,
                    |zoom, fixed_ratio| message::Message::ZoomXChanged { zoom, fixed_ratio },
                    |zoom, fixed_ratio| message::Message::ZoomYChanged { zoom, fixed_ratio },
                )
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        let main_content = column![
            self.titlebar.view(&self.window),
            row![left_bar, main_area].height(Length::Fill),
            self.statusbar.view(),
        ];

        container(main_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(iced_core::Color::TRANSPARENT)),
                ..Default::default()
            })
            .into()
    }

    /// 获取当前需要绘制的音符实例
    pub fn get_note_instances(&self) -> Vec<NoteInstance> {
        let sidebar_width = self.sidebar.width() as f32;
        let mut instances = self
            .editor
            .get_note_instances(&self.window.theme, sidebar_width);

        // 添加洋葱皮音符（其他音轨的音符）
        let onion_states = self.sidebar.get_onion_skin_states();
        let onion_instances = self.editor.get_all_onion_skin_instances(&onion_states);
        instances.extend(onion_instances);

        instances
    }
}
