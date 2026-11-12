//! 工程走带视图 — view_arrangement

use iced_core::Length;
use iced_widget::{column, container};
use lumino_ui_core::constants::editor::ARRANGEMENT_RULER_HEIGHT;

use crate::root::Root;
use crate::{Element, Theme};

impl Root {
    /// 渲染工程走带视图
    ///
    /// 左侧音轨列表 + 右侧 wgpu 渲染区域（`iced` 侧只做布局占位）。
    ///
    /// 轨道列 / 标尺 / 泳道输入全部由 egui 接管；wgpu 负责画网格与音符，
    /// 其输出经 egui 外部纹理合成。
    pub(crate) fn view_arrangement(&self, _available_width: f32) -> Element<'_> {
        puffin::profile_scope!("root_view_arrangement");

        const TRACK_LIST_WIDTH: f32 = 160.0;

        // 轨道列 / 滚动条 / 加轨按钮均由 egui 绘制并接管输入，iced 只保留占位。

        // 轨道列由 egui 绘制（`ui-egui` 的 arrangement::track_list）；
        // iced 只保留同尺寸占位，保证布局与命中基准不变。
        let track_list = iced_widget::Space::new()
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fill);

        // 添加音轨按钮由 egui 绘制并接管（`ui-egui` 的 arrangement::scrollbars），
        // iced 只保留同尺寸占位。

        // 左侧栏：轨道列占位与右侧走带区域对齐（绘制在 egui 侧）。

        // 右侧走带区域：wgpu 渲染可见（空容器不加背景色），输入由 egui 处理。
        // 走带主体（泳道）由 wgpu 渲染、输入由 egui 接管；iced 侧只保留占位。
        let arrangement_area = iced_widget::container(iced_widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill);

        // 水平滚动条：由 egui 绘制并接管输入（iced 只保留 20px 占位，
        // 保持与 `arrangement_geometry` 完全一致的布局）。
        let h_scrollbar: Element<'_> = iced_widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(20.0))
            .into();

        // 垂直滚动条：egui 接管，iced 保留 20px 宽占位。
        let v_scrollbar: Element<'_> = iced_widget::Space::new()
            .width(Length::Fixed(20.0))
            .height(Length::Fill)
            .into();

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

        // 底部行：添加音轨按钮（左侧，与音轨列表等宽）+ 水平滚动条（右侧填充）
        // 两者高度均为 20px；按钮由 egui 绘制并接管，iced 只保留同尺寸占位。
        let add_track_btn: Element<'_> = iced_widget::Space::new()
            .width(Length::Fixed(TRACK_LIST_WIDTH))
            .height(Length::Fixed(20.0))
            .into();
        let bottom_row = iced_widget::row![add_track_btn, h_scrollbar]
            .width(Length::Fill)
            .height(Length::Fixed(20.0));

        column![
            self.toolbar.toolbar_view(),
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
