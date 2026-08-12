//! 主视图渲染函数
//!
//! 包含 Root 主入口视图、主窗口渲染、工程走带视图和瀑布流占位页面。

use iced_core::Length;
use iced_widget::{button, column, container, row, scrollable, text};

use super::right_content;
use crate::message;
use crate::resources::icon::{self, Icon};
use crate::right_sidebar;
use crate::root::Root;
use crate::sidebar::Event as SidebarEvent;
use crate::view::audio_export_dialog::view_audio_export_dialog;
use crate::view::video_export_dialog::view_video_export_dialog;
use crate::{Element, Theme};

impl Root {
    /// 渲染视图（主入口，根据窗口类型分发）
    pub(super) fn root_view(&self) -> Element<'_> {
        puffin::profile_scope!("root_view");

        if self.is_progress_window {
            self.view_progress()
        } else if self.state.is_dialog_window {
            self.view_dialog()
        } else {
            self.view_main()
        }
    }

    /// 右侧栏是否应随钢琴卷帘编辑区一起渲染
    ///
    /// 右侧栏只属于钢琴卷帘编辑区：进入工程走带 / 瀑布流 / 音频视频导出面板
    /// 或关闭钢琴卷帘（钢琴卷帘 UI 隐藏）时，右侧栏跟随隐藏。
    /// 视图层调用此函数决定是否渲染右侧栏组件——所有"非钢琴卷帘"视图
    /// （走带、瀑布流、导出面板、卷帘关闭）均不得渲染右侧栏。
    pub(crate) fn right_sidebar_visible(&self) -> bool {
        self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
            && self.sidebar.piano_roll_visible
            && !self.sidebar.is_arrangement_route()
            && !self.sidebar.audio_export_visible
            && !self.sidebar.video_export_visible
    }

    /// 渲染主窗口
    pub(super) fn view_main(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_main");

        let is_arrangement_route = self.sidebar.is_arrangement_route();

        // 左侧栏（包含图标栏和音轨面板）
        puffin::profile_scope!("root_view_sidebar");
        let ppq = self.editor.editor_state.view.ppq;
        let left_bar = self.sidebar.view(
            &self.window,
            self.settings.language,
            self.state.current_mode,
            self.toolbar.note_precision.as_ticks(ppq),
        );

        // 右侧内容区域（工具栏 + 编辑器 + 力度面板 / 瀑布流占位）
        puffin::profile_scope!("root_view_right_content");
        let right_content: Element<'_> = if self.state.current_mode
            == crate::titlebar::mode_toggle::AppMode::Waterfall
        {
            // 瀑布流模式：显示"实现中"占位页面
            self.view_waterfall_placeholder()
        } else if is_arrangement_route {
            // 音轨总览模式：使用 wgpu 原生渲染
            right_content::wrap_right_content(self, false, true, |available_width| {
                self.view_arrangement(available_width)
            })
        } else if self.sidebar.audio_export_visible {
            // 音频渲染面板（在主界面钢琴卷帘区域显示）
            self.view_audio_export_panel()
        } else if self.sidebar.video_export_visible {
            // 视频渲染面板（在主界面钢琴卷帘区域显示）
            self.view_video_export_panel()
        } else if !self.sidebar.piano_roll_visible {
            // 钢琴卷帘已关闭：显示空白区域
            container(
                iced_widget::column![]
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
        } else {
            // 钢琴卷帘编辑区 —— 右侧栏唯一渲染位置。
            // 右侧栏跟随钢琴卷帘 UI 显隐（right_sidebar_visible 收口）：
            // 离开钢琴卷帘（工程走带/瀑布流/导出面板/卷帘关闭）时由上方
            // 各分支接管，不渲染右侧栏。
            let has_selection = self.editor.selected_notes_count() > 0;
            right_content::wrap_right_content(self, has_selection, false, move |available_width| {
                let velocity_panel = if self.sidebar.automation_panel_visible {
                    self.editor.velocity_panel.view(
                        &self.editor,
                        self.visual.velocity_panel_height,
                        self.settings.language,
                    )
                } else {
                    iced_widget::Space::new().height(0).into()
                };
                let editor_view = self.editor.view(
                    message::Message::ScrollbarScrolled,
                    message::Message::ScrollbarScrolledY,
                    |zoom, fixed_ratio| message::Message::ZoomXChanged { zoom, fixed_ratio },
                    |zoom, fixed_ratio| message::Message::ZoomYChanged { zoom, fixed_ratio },
                );
                let perf_ctx = crate::toolbar::ToolbarPerfContext {
                    playback_tick: self.editor.playback_position,
                    ppq: self.editor.editor_state.view.ppq,
                    tempo_points: &self.editor.editor_state.data.tempo_points,
                };
                let toolbar = self.toolbar.toolbar_view(
                    &self.window,
                    has_selection,
                    self.settings.language,
                    &perf_ctx,
                    available_width,
                    false,
                );
                // 右侧栏渲染条件收口：仅钢琴卷帘编辑区渲染（防御性兜底，
                // 正常情况下该分支即满足 right_sidebar_visible）
                let right_bar = if self.right_sidebar_visible() {
                    right_sidebar::view::view(
                        &self.right_sidebar,
                        &self.window,
                        self.settings.language,
                    )
                } else {
                    iced_widget::Space::new().into()
                };
                column![
                    toolbar,
                    row![
                        column![container(editor_view).height(Length::Fill), velocity_panel,]
                            .height(Length::Fill),
                        right_bar,
                    ]
                    .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            })
        };

        puffin::profile_scope!("root_view_main_content");
        let main_content = if cfg!(target_os = "macos") {
            column![
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        } else {
            // 导出为素材的启用条件：卷帘选中音符 或 走带视图跨音轨框选
            let export_material_enabled = self.editor.selected_notes_count() > 0
                || !self.editor.editor_state.data.arrange_selection.is_empty();
            column![
                self.titlebar.view(
                    &self.window,
                    self.settings.use_native_titlebar,
                    self.state.current_mode,
                    self.state.toggle_animation.position,
                    self.settings.language,
                    export_material_enabled,
                ),
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        };

        // 叠加 Toast 通知层（右下角）
        if let Some(toast_overlay) = self.toast.view(&self.window.theme) {
            iced_widget::Stack::new()
                .push(main_content)
                .push(toast_overlay)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            main_content.into()
        }
    }

    /// 渲染工程走带视图
    ///
    /// 左侧音轨列表（Canvas）+ 右侧 wgpu 渲染区域。
    /// 音符由 WGPU ArrangementRenderer 绘制，不再使用 CPU 端 Canvas 预计算。
    pub(super) fn view_arrangement(&self, available_width: f32) -> Element<'_> {
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
            .map(|track| (track.id, track.name.clone()))
            .collect();

        // 收集每轨的显示标签、通道号、Conductor 标识等元数据，
        // 确保走带视图的通道标签与侧边栏一致。
        let track_labels: Vec<String> = self
            .sidebar
            .tracks
            .iter()
            .map(|track| track.display_label.clone())
            .collect();
        let track_channels: Vec<u8> = self
            .sidebar
            .tracks
            .iter()
            .map(|track| track.channel)
            .collect();
        let track_conductors: Vec<bool> = self
            .sidebar
            .tracks
            .iter()
            .map(|track| track.is_conductor)
            .collect();

        let track_list_canvas = crate::editor::arrangement::TrackListCanvas::new(
            track_data,
            self.sidebar.selected_track,
            vp.scroll_y,
            TRACK_HEIGHT * vp.zoom_y,
            total_height,
        )
        .with_labels(track_labels)
        .with_channels(track_channels)
        .with_conductors(track_conductors)
        // 长按激活拖拽排序（Sidebar 统一计时），驱动走带指示线与遮罩绘制
        .with_drag_active(
            self.sidebar
                .track_reorder
                .as_ref()
                .is_some_and(|r| r.active),
        );
        let track_list = iced_widget::canvas::Canvas::new(track_list_canvas)
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fill);

        // 添加音轨按钮（放在音轨列表底部，参考 yinhe 的 "+" 角落按钮）
        let add_track_btn = button(
            container(icon::view_with_size_and_theme(
                Icon::Plus,
                16,
                16,
                Some(&self.window.theme),
            ))
            .width(Length::Fill)
            .height(Length::Fixed(20.0))
            .align_x(iced_core::alignment::Horizontal::Center)
            .align_y(iced_core::alignment::Vertical::Center),
        )
        .width(Length::Fixed(TRACK_LIST_WIDTH))
        .height(Length::Fixed(20.0))
        .on_press(SidebarEvent::add_track())
        .style(|theme: &Theme, status| {
            let palette = theme.extended_palette();
            let bg = if status == iced_widget::button::Status::Hovered {
                palette.background.weak.color
            } else {
                palette.background.base.color
            };
            iced_widget::button::Style {
                text_color: palette.background.base.text,
                border: iced_core::Border {
                    radius: 0.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        });

        // 左侧栏：音轨列表 Canvas 直接填满高度，与右侧走带区域对齐。
        // add_track_btn 移至底部水平滚动条行，避免占用 Canvas 高度导致最后一轨截断。

        // 右侧走带区域 — 由 WGPU ArrangementRenderer 渲染
        // 使用空容器作为占位，不设置背景色，让 wgpu 渲染可见
        // 上方叠加透明 Canvas 捕获点击事件以移动演奏指示线
        let track_count = self.sidebar.tracks.len();
        let arr_sel_rect = self
            .editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .first()
            .map(|&(ts, te, _kl, _kh, tl, th)| (ts as f64, te as f64, tl as usize, th as usize));
        let click_canvas = crate::editor::arrangement::ArrangementClickCanvas {
            viewport: vp.clone(),
            current_tool: self.editor.current_tool(),
            track_count,
            arr_sel_rect,
            selected_notes: self.editor.arrangement_selected_notes(),
            ppq: self.editor.editor_state.view.ppq,
            precision: self.toolbar.note_precision,
            time_signatures: self.editor.editor_state.data.time_signatures.clone(),
            ctrl_pressed: self.toolbar.ctrl_pressed,
            shift_pressed: self.toolbar.shift_pressed,
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

        // 水平滚动条：使用 cached_max_tick_end（已由 arrangement_max_tick_end() 更新），
        // 回退到 total_ticks 确保至少 4 小节宽度
        let max_tick_val = vp
            .cached_max_tick_end
            .max(vp.total_ticks as f32)
            .max(960.0 * 4.0);
        let total_width = max_tick_val * ppu;
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

        let perf_ctx = crate::toolbar::ToolbarPerfContext {
            playback_tick: self.editor.playback_position,
            ppq: self.editor.editor_state.view.ppq,
            tempo_points: &self.editor.editor_state.data.tempo_points,
        };
        // 底部行：添加音轨按钮（左侧，与音轨列表等宽）+ 水平滚动条（右侧填充）
        // 两者高度均为 20px，与原 h_scrollbar 占用空间一致，不影响 viewport 计算
        let bottom_row = iced_widget::row![add_track_btn, h_scrollbar]
            .width(Length::Fill)
            .height(Length::Fixed(20.0));

        column![
            self.toolbar.toolbar_view(
                &self.window,
                false,
                self.settings.language,
                &perf_ctx,
                available_width,
                true,
            ),
            // 走带视图不渲染右侧栏：右侧栏只属于钢琴卷帘编辑区，
            // 跟随钢琴卷帘 UI 显隐（见 right_sidebar_visible）。
            arrangement_row.height(Length::Fill),
            bottom_row,
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染音频渲染面板（在主界面钢琴卷帘区域显示）
    pub(super) fn view_audio_export_panel(&self) -> Element<'_> {
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
    pub(super) fn view_video_export_panel(&self) -> Element<'_> {
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
    pub(super) fn view_waterfall_placeholder(&self) -> Element<'_> {
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
