//! Root 视图渲染子模块

use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, text};
use lumino_gfx::NoteInstance;

use crate::root::{Element, Root, Theme};
use crate::state::root_state::DialogType;
use crate::view::{
    collaboration_dialog::view_collaboration_dialog,
    custom_precision_dialog::view_custom_precision_dialog,
    load_confirm_dialog::view_load_confirm_dialog,
};
use crate::{message, settings};

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
            DialogType::LoadConfirm => {
                view_load_confirm_dialog(&self.state.load_confirm_dialog, &self.window.theme)
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
            settings::view(&self.settings, &self.window, &self.state.system_fonts)
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

        let main_content = if cfg!(target_os = "macos") {
            column![
                row![left_bar, main_area].height(Length::Fill),
                self.statusbar.view(),
            ]
        } else {
            column![
                self.titlebar
                    .view(&self.window, self.settings.use_native_titlebar),
                row![left_bar, main_area].height(Length::Fill),
                self.statusbar.view(),
            ]
        };

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
    pub fn update_note_instances(&mut self, instances: &mut Vec<NoteInstance>) {
        let sidebar_width = self.sidebar.width() as f32;
        self.editor
            .update_note_instances(&self.window.theme, sidebar_width, instances);

        // 计算可见区域用于洋葱皮音符的视锥裁剪
        let es = &self.editor.editor_state;
        let view = &es.view;
        let canvas_size = es.canvas.size;
        let viewport_width = canvas_size.x - view.keyboard_width;
        let viewport_height = canvas_size.y - view.ruler_height;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        let max_key_index = (view.visible_key_count - 1) as f32;
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let visible_key_max = key_top_f32.ceil() as u16 + 1;
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1);

        let onion_states = self.sidebar.get_onion_skin_states();
        let notes: Vec<(f32, u16, f32, iced_core::Color)> = self.editor.get_onion_skin_notes(
            &onion_states,
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        );

        for (tick, key, length, color) in notes {
            let note = crate::editor::note::Note::new(tick, key, length);
            let instance = note.to_instance(color);
            instances.push(instance);
        }
    }

    /// 获取网格线实例（用于 wgpu 渲染）
    pub fn update_grid_line_instances(&self, instances: &mut Vec<lumino_gfx::GridLineInstance>) {
        use crate::editor::grid::theme::ThemeExt;

        // 从主题获取颜色
        let bar_color = self.window.theme.bar_line_color();
        let beat_color = self.window.theme.beat_line_color();
        let half_beat_color = self.window.theme.half_beat_line_color();
        let grid_color = self.window.theme.grid_line_color();

        // 琴键分隔线颜色
        let _palette = self.window.theme.extended_palette().background;
        let key_line_color = if self.window.theme.is_light() {
            iced_core::Color {
                a: 0.2,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.2,
                ..iced_core::Color::WHITE
            }
        };

        self.editor.update_grid_line_instances(
            bar_color,
            beat_color,
            half_beat_color,
            grid_color,
            key_line_color,
            instances,
        );
    }
}
