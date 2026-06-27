//! Root 视图渲染子模块

use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, space, text};
use lumino_gfx::NoteInstance;

use crate::editor::note::NoteExt;
use crate::message;
use crate::root::{Element, Root, Theme};
use crate::state::root_state::DialogType;
use crate::view::{
    audio_export_dialog::view_audio_export_dialog, collaboration_dialog::view_collaboration_dialog,
    custom_precision_dialog::view_custom_precision_dialog,
    load_confirm_dialog::view_load_confirm_dialog,
    project_settings_dialog::view_project_settings_dialog, settings_dialog::view_settings_dialog,
    speed_change_dialog::view_speed_change_dialog,
};

impl Root {
    /// 渲染视图
    pub fn view(&self) -> Element<'_> {
        puffin::profile_scope!("root_view");

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
    fn view_dialog(&self) -> Element<'_> {
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
            DialogType::AudioExport => {
                view_audio_export_dialog(&self.state.audio_export_dialog, &self.window.theme)
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

    /// 渲染主窗口
    fn view_main(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_main");

        let is_arrangement_route = self.sidebar.is_arrangement_route();

        // 左侧栏（包含图标栏和音轨面板）
        puffin::profile_scope!("root_view_sidebar");
        let left_bar = self.sidebar.view(&self.window, self.settings.language);

        // 右侧内容区域（工具栏 + 编辑器 + 力度面板 / 瀑布流占位）
        puffin::profile_scope!("root_view_right_content");
        let right_content: Element<'_> =
            if self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall {
                // 瀑布流模式：显示"实现中"占位页面
                self.view_waterfall_placeholder()
            } else if is_arrangement_route {
                // 音轨总览模式：使用 wgpu 原生渲染
                self.view_arrangement()
            } else {
                // 自动化面板（力度/CC/Tempo/Bend 绘制面板）
                // 由侧边栏自动化按钮控制显示/隐藏
                let velocity_panel: Element<'_> = if self.sidebar.automation_panel_visible {
                    self.editor.velocity_panel.view(
                        &self.editor,
                        self.visual.velocity_panel_height,
                        self.settings.language,
                    )
                } else {
                    iced_widget::Space::new().height(0).into()
                };
                // 编辑器视图（卷帘 + 滚动条）
                let editor_view = self.editor.view(
                    message::Message::ScrollbarScrolled,
                    message::Message::ScrollbarScrolledY,
                    |zoom, fixed_ratio| message::Message::ZoomXChanged { zoom, fixed_ratio },
                    |zoom, fixed_ratio| message::Message::ZoomYChanged { zoom, fixed_ratio },
                );
                column![
                    self.toolbar.view(
                        &self.window,
                        self.editor.selected_notes_count() > 0,
                        self.settings.language
                    ),
                    column![container(editor_view).height(Length::Fill), velocity_panel,]
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            };

        puffin::profile_scope!("root_view_main_content");
        let main_content = if cfg!(target_os = "macos") {
            column![
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        } else {
            column![
                self.titlebar.view(
                    &self.window,
                    self.settings.use_native_titlebar,
                    self.state.current_mode,
                    self.state.toggle_animation.position,
                    self.settings.language,
                ),
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        };

        // 性能面板展开时，使用 Stack 将面板作为浮动层渲染在状态栏上方
        if self.statusbar.perf_panel_expanded {
            puffin::profile_scope!("root_view_perf_panel");
            let perf_data = self.statusbar.perf_data();
            let panel = crate::statusbar::performance::performance_panel_view(perf_data);

            iced_widget::Stack::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .push(
                    container(main_content)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .push(
                    column![
                        iced_widget::Space::new().height(Length::Fill),
                        container(panel).padding(iced_core::Padding {
                            top: 0.0,
                            right: 0.0,
                            bottom: 20.0,
                            left: 0.0,
                        }),
                    ]
                    .width(Length::Fill),
                )
                .into()
        } else {
            container(main_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(iced_core::Color::TRANSPARENT)),
                    ..Default::default()
                })
                .into()
        }
    }

    /// 获取当前需要绘制的音符实例
    pub fn update_note_instances(&mut self, instances: &mut Vec<NoteInstance>) {
        let sidebar_width = self.sidebar.width() as f32;
        self.editor
            .update_note_instances(&self.window.theme, sidebar_width, instances);

        // 计算可见区域用于洋葱皮音符的视锥裁剪
        let es = &self.editor.editor_state;
        let view = &es.view;
        let canvas_size = es.canvas.size_x;
        let viewport_width = canvas_size - view.keyboard_width;
        let viewport_height = es.canvas.size_y - view.ruler_height;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let _visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        let max_key_index = (view.visible_key_count - 1) as f32;
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let _visible_key_max = key_top_f32.ceil() as u16 + 1;
        let _visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1);

        let notes: Vec<(f32, u16, f32, iced_core::Color)> = Vec::new();

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

    /// 渲染状态栏（性能面板已交由 Stack 浮动层处理）
    fn view_status_section(&self) -> Element<'_> {
        self.statusbar.view(self.settings.language)
    }

    /// 渲染工程走带视图
    ///
    /// 左侧音轨列表（Canvas）+ 右侧 wgpu 渲染区域。
    /// 音符由 WGPU ArrangementRenderer 绘制，不再使用 CPU 端 Canvas 预计算。
    fn view_arrangement(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_arrangement");

        const TRACK_LIST_WIDTH: f32 = 160.0;
        const TRACK_HEIGHT: f32 = 48.0;

        let track_count = self.sidebar.tracks.len();
        let vp = &self.arrangement_view.viewport;
        let total_height = track_count as f32 * TRACK_HEIGHT * vp.zoom_y;
        let ppu = vp.zoom_x; // pixels_per_tick

        // 左侧音轨列表 Canvas（与走带区域共享 scroll_y，实现同步滚动）
        let track_data: Vec<(usize, String)> = self
            .sidebar
            .tracks
            .iter()
            .map(|t| (t.id, t.name.clone()))
            .collect();
        let track_list_canvas = crate::editor::arrangement::TrackListCanvas::new(
            track_data,
            self.sidebar.selected_track,
            vp.scroll_y,
            TRACK_HEIGHT * vp.zoom_y,
            total_height,
        );
        let track_list = iced_widget::canvas::Canvas::new(track_list_canvas)
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fill);

        // 右侧走带区域 — 由 WGPU ArrangementRenderer 渲染
        // 使用空容器作为占位，不设置背景色，让 wgpu 渲染可见
        // 上方叠加透明 Canvas 捕获点击事件以移动演奏指示线
        let click_canvas = crate::editor::arrangement::ArrangementClickCanvas {
            viewport: vp.clone(),
        };
        let arrangement_area = iced_widget::Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(
                iced_widget::container(
                    iced_widget::column![]
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .push(
                iced_widget::canvas::Canvas::new(click_canvas)
                    .width(Length::Fill)
                    .height(Length::Fill),
            );

        // 水平滚动条
        let total_ticks_val = vp.total_ticks.max(960 * 4); // 至少 4 小节
        let total_width = total_ticks_val as f32 * ppu;
        let max_scroll_x = total_width.max(vp.canvas_size.x);
        let h_scrollbar = crate::editor::scrollbar_widget::ScrollbarWidget::horizontal(
            vp.scroll_x,
            max_scroll_x,
            vp.zoom_x,
            Some(vp.canvas_size.x),
            crate::Message::ArrangementScrollX,
            |zoom, ratio| crate::Message::ArrangementZoomX {
                zoom,
                fixed_ratio: ratio,
            },
        );

        // 垂直滚动条
        let v_scrollbar = crate::editor::scrollbar_widget::ScrollbarWidget::vertical(
            vp.scroll_y,
            total_height,
            vp.zoom_y,
            Some(vp.canvas_size.y.max(1.0)),
            crate::Message::ArrangementScrollY,
            |zoom, ratio| crate::Message::ArrangementZoomY {
                zoom,
                fixed_ratio: ratio,
            },
        );

        let arrangement_row = iced_widget::row![track_list, arrangement_area, v_scrollbar,];

        column![
            self.toolbar
                .view(&self.window, false, self.settings.language),
            arrangement_row.height(Length::Fill),
            h_scrollbar,
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染瀑布流模式占位页面（功能实现中）
    fn view_waterfall_placeholder(&self) -> Element<'_> {
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
