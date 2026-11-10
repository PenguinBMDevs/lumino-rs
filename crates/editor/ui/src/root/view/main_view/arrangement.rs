//! 工程走带视图 — view_arrangement

use iced_core::Length;
use iced_widget::{button, column, container};
use lumino_ui_core::constants::editor::ARRANGEMENT_RULER_HEIGHT;

use crate::resources::icon::{self, Icon};
use crate::root::Root;
use crate::sidebar::Event as SidebarEvent;
use crate::{Element, Theme};

impl Root {
    /// 渲染工程走带视图
    ///
    /// 左侧音轨列表 + 右侧 wgpu 渲染区域（`iced` 侧只做布局占位）。
    ///
    /// 轨道列 / 标尺 / 泳道输入全部由 egui 接管；wgpu 负责画网格与音符，
    /// 其输出经 egui 外部纹理合成。
    pub(crate) fn view_arrangement(&self, available_width: f32) -> Element<'_> {
        puffin::profile_scope!("root_view_arrangement");

        const TRACK_LIST_WIDTH: f32 = 160.0;
        const TRACK_HEIGHT: f32 = 48.0;

        let track_count = self.sidebar.tracks.len();
        let vp = &self.arrangement_view.viewport;
        let total_height = track_count as f32 * TRACK_HEIGHT * vp.zoom_y;
        let ppu = vp.zoom_x; // pixels_per_tick

        // 轨道列由 egui 绘制（`ui-egui` 的 arrangement::track_list）；
        // iced 只保留同尺寸占位，保证布局与命中基准不变。
        let track_list = iced_widget::Space::new()
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

        // 左侧栏：轨道列占位与右侧走带区域对齐（绘制在 egui 侧）。

        // 右侧走带区域：wgpu 渲染可见（空容器不加背景色），输入由 egui 处理。
        // 走带主体（泳道）由 wgpu 渲染、输入由 egui 接管；iced 侧只保留占位。
        let arrangement_area = iced_widget::container(iced_widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill);

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

        // 顶部时间标尺：走带主体是 WGPU 线框（无文字层），这里补 28px Canvas
        // 小节号标尺。它与下方走带区共用同一个 `scroll_x/zoom_x`，点击/拖拽
        // 发 `ArrangementCursorSet`，滚轮缩放与钢琴卷帘标尺一致。
        // 顶部标尺由 egui 绘制；iced 侧占位保持 28px 高度。
        let ruler = iced_widget::container(iced_widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fixed(ARRANGEMENT_RULER_HEIGHT));

        // 三列顶部各留 28px：音轨列表/走带区/垂直滚动条从标尺下方开始，
        // 保证 lane 行与标尺刻度在同一 y 基准上。左右两角用同色背景补齐，
        // 避免标尺条两侧出现透明缝。
        let header_style = |theme: &Theme| container::Style {
            background: Some(theme.extended_palette().background.weak.color.into()),
            ..container::Style::default()
        };
        let track_list_top = container(iced_widget::Space::new())
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fixed(ARRANGEMENT_RULER_HEIGHT))
            .style(header_style);
        let scrollbar_top = container(iced_widget::Space::new())
            .width(Length::Fixed(20.0))
            .height(Length::Fixed(ARRANGEMENT_RULER_HEIGHT))
            .style(header_style);

        let track_list_column = iced_widget::column![track_list_top, track_list,]
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fill);
        let arrangement_column = iced_widget::column![ruler, arrangement_area]
            .width(Length::Fill)
            .height(Length::Fill);
        let v_scrollbar_column =
            iced_widget::column![scrollbar_top, v_scrollbar,].height(Length::Fill);

        let arrangement_row =
            iced_widget::row![track_list_column, arrangement_column, v_scrollbar_column];

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
