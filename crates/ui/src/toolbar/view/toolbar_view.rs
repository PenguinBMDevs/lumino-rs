//! 工具栏主视图函数
//!
//! 组装各子模块提供的渲染方法，构建完整的工具栏布局。

use iced_core::Alignment;
use iced_widget::{column, container, row, space};

use crate::toolbar::{RESIZE_HANDLE_HEIGHT, Toolbar, ToolbarPerfContext};
use crate::{Element, Theme, window};

impl Toolbar {
    /// 渲染工具栏主视图
    ///
    /// 调用各子模块的渲染方法构建完整工具栏布局，包含：
    /// - 录制按钮（record.rs）
    /// - 播放控制、循环、工具、调整手柄、撤销/重做（controls.rs）
    /// - 精度选择、自动滚动、协作（status.rs）
    pub fn toolbar_view<'a>(
        &'a self,
        window: &'a window::Window,
        has_selection: bool,
        language: lumino_core::i18n::Language,
        perf: &ToolbarPerfContext<'_>,
    ) -> Element<'a> {
        let t = lumino_core::i18n::main_translations(language);
        let palette = window.theme.extended_palette();

        // 计算内容区域高度（总高度减去手柄高度）
        let content_height = self.height - RESIZE_HANDLE_HEIGHT;

        // 各区域渲染
        let record_button = self.render_record_button(content_height, palette, window, language);

        let playback_controls = self.render_playback_controls(content_height, palette, t, window);

        let loop_button = self.render_loop_button(content_height, palette, t, window);

        let undo_redo_controls = self.render_undo_redo_controls(content_height, palette, t, window);

        let tools = self.render_tools_section(content_height, palette, has_selection, t, window);

        let precision_selector =
            self.render_precision_selector(content_height, palette, language, t);

        let auto_scroll_button = self.render_auto_scroll_button(content_height, palette, t, window);

        let collaboration_button =
            self.render_collaboration_button(content_height, palette, t, window);

        let dashboard = self.render_detection_dashboard(
            content_height,
            palette,
            perf.perf_data,
            perf.playback_tick,
            perf.ppq,
            perf.tempo_points,
        );

        let resize_handle = self.render_resize_handle(palette);

        // 主工具栏内容 - 横向排列所有区域，协作按钮在最右边
        let toolbar_content = container(
            row![
                record_button,
                space().width(4),
                playback_controls,
                space().width(8),
                loop_button,
                space().width(8),
                undo_redo_controls,
                space().width(16),
                tools,
                space().width(iced_widget::core::Length::Fill),
                auto_scroll_button,
                space().width(16),
                precision_selector,
                space().width(16),
                dashboard,
                space().width(16),
                collaboration_button,
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Fill)
        .height(iced_widget::core::Length::Fixed(content_height))
        .padding([8, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 组合工具栏内容和调整手柄
        column![toolbar_content, resize_handle]
            .width(iced_widget::core::Length::Fill)
            .height(iced_widget::core::Length::Fixed(self.height))
            .into()
    }
}
