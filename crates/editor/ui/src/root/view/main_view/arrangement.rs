//! 工程走带视图 — view_arrangement

use iced_core::Length;
use iced_widget::{button, column, container};

use crate::resources::icon::{self, Icon};
use crate::root::Root;
use crate::sidebar::Event as SidebarEvent;
use crate::{Element, Theme};

impl Root {
    /// 渲染工程走带视图
    ///
    /// 左侧音轨列表（Canvas）+ 右侧 wgpu 渲染区域。
    /// 音符由 WGPU ArrangementRenderer 绘制，不再使用 CPU 端 Canvas 预计算。
    pub(crate) fn view_arrangement(&self, available_width: f32) -> Element<'_> {
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
        )
        // Ctrl+滚轮垂直缩放：同步走带视口 zoom_y 与窗口级 Ctrl 键状态
        .with_zoom_y(vp.zoom_y)
        .with_ctrl_pressed(self.toolbar.ctrl_pressed);
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
                self.settings.display.language,
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
}
