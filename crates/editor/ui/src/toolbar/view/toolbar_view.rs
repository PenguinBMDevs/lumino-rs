//! 工具栏主视图函数
//!
//! 组装各子模块提供的渲染方法，构建完整的工具栏布局。
//!
//! # 折叠与「更多」按钮
//!
//! 布局拆成两段：**内容区**（左侧分组 + 让位空格 + 右侧分组）与**「更多」按钮**。
//! 内容区宽度不足时由 `compute_overflow` 提前折掉条目/分组（见
//! `toolbar/overflow/interaction.rs`），并为其加裁剪兜底；「更多」按钮在裁剪容器
//! **之外**、以固定宽度渲染——它的尺寸不参与压缩，也永远不会被内容顶出可视区。

use iced_core::{Alignment, Length};
use iced_widget::{Column, Row, container, row, space};

use crate::toolbar::overflow::{MORE_BUTTON_WIDTH, MORE_GAP, ToolItem, ToolbarGroup};
use crate::toolbar::{ButtonId, RESIZE_HANDLE_HEIGHT, Toolbar, ToolbarPerfContext};
use crate::{Element, Theme, window};

impl Toolbar {
    /// 渲染工具栏主视图
    ///
    /// 调用各子模块的渲染方法构建完整工具栏布局，包含：
    /// - 录制按钮（record.rs）
    /// - 播放控制、循环、工具、调整手柄、撤销/重做（controls.rs）
    /// - 精度选择器已并入工具选择区框内（controls.rs，内容定义于 status.rs）
    /// - 洋葱皮全局显示开关、自动滚动、协作（onion_skin.rs / status.rs）
    /// - 检测仪表盘（detection_dashboard.rs）
    ///
    /// 新增 `available_width` 用于自动折叠：宽度不足时，低优先级分组会被收到
    /// “更多”(⋮) 按钮的弹出菜单中；**主工具栏内部**则按条目逐个折叠。
    pub fn toolbar_view<'a>(
        &'a self,
        window: &'a window::Window,
        has_selection: bool,
        language: lumino_extras::i18n::Language,
        perf: &ToolbarPerfContext<'_>,
        available_width: f32,
        arrangement_mode: bool,
    ) -> Element<'a> {
        let t = lumino_extras::i18n::main_translations(language);
        let palette = window.theme.extended_palette();

        // 计算内容区域高度（总高度减去手柄高度）
        let content_height = self.height - RESIZE_HANDLE_HEIGHT;

        let overflow = self.compute_overflow(available_width, arrangement_mode);
        let has_overflow = overflow.has_overflow();
        let visible_groups = &overflow.visible_groups;
        let visible_tools = &overflow.visible_tools;

        let mut toolbar_elements: Vec<Element<'a>> = Vec::new();
        let right_visible = ToolbarGroup::RIGHT
            .iter()
            .any(|g| visible_groups.contains(g));

        // 左侧分组：Record / Playback / Loop / UndoRedo / Dashboard / Tools
        self.push_left_groups(
            &mut toolbar_elements,
            visible_groups,
            right_visible || has_overflow,
            content_height,
            palette,
            has_selection,
            t,
            window,
            language,
            perf,
            arrangement_mode,
            visible_tools,
        );

        // 右侧分组：OnionSkin / AutoScroll / Collaboration
        self.push_right_groups(
            &mut toolbar_elements,
            visible_groups,
            content_height,
            palette,
            t,
            window,
            arrangement_mode,
            visible_tools,
        );

        // 内容区：折叠时加裁剪兜底（估算偏小也不会把「更多」推出可视区）
        let content_area =
            container(Row::with_children(toolbar_elements).align_y(Alignment::Center))
                .width(Length::Fill)
                .clip(has_overflow);

        // 「更多」按钮：固定宽度，独立于内容区（不得被压缩/裁剪）
        let mut bar_items: Vec<Element<'a>> = vec![content_area.into()];
        if has_overflow {
            bar_items.push(space().width(MORE_GAP).into());
            bar_items.push(self.render_more_button(content_height, palette, t, window));
        }

        let toolbar_content = container(row(bar_items).align_y(Alignment::Center))
            .width(Length::Fill)
            .height(Length::Fixed(content_height))
            .padding([8, 16])
            .style(move |_theme: &Theme| {
                container::Style::default().background(palette.background.weakest.color)
            });

        let resize_handle = self.render_resize_handle(palette);

        // 组合工具栏内容和调整手柄
        Column::with_children(vec![toolbar_content.into(), resize_handle])
            .width(Length::Fill)
            .height(Length::Fixed(self.height))
            .into()
    }

    /// 将左侧分组按顺序加入工具栏元素列表
    #[allow(clippy::too_many_arguments)]
    fn push_left_groups<'a>(
        &'a self,
        elements: &mut Vec<Element<'a>>,
        visible_groups: &[ToolbarGroup],
        has_right_or_more: bool,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        has_selection: bool,
        t: &'static lumino_extras::i18n::MainTranslations,
        window: &'a window::Window,
        language: lumino_extras::i18n::Language,
        perf: &ToolbarPerfContext<'_>,
        arrangement_mode: bool,
        visible_tools: &[ToolItem],
    ) {
        let left_groups = ToolbarGroup::LEFT;
        for (i, group) in left_groups.iter().enumerate() {
            if !visible_groups.contains(group) {
                continue;
            }

            elements.push(self.render_group(
                *group,
                content_height,
                palette,
                has_selection,
                t,
                window,
                language,
                perf,
                arrangement_mode,
                visible_tools,
            ));

            let is_last_left = left_groups[i + 1..]
                .iter()
                .all(|g| !visible_groups.contains(g));
            if is_last_left && has_right_or_more {
                elements.push(space().width(Length::Fill).into());
            } else if !is_last_left {
                elements.push(space().width(group.spacing_after()).into());
            }
        }
    }

    /// 将右侧分组按顺序加入工具栏元素列表
    #[allow(clippy::too_many_arguments)]
    fn push_right_groups<'a>(
        &'a self,
        elements: &mut Vec<Element<'a>>,
        visible_groups: &[ToolbarGroup],
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static lumino_extras::i18n::MainTranslations,
        window: &'a window::Window,
        arrangement_mode: bool,
        visible_tools: &[ToolItem],
    ) {
        let right_groups = ToolbarGroup::RIGHT;
        for (i, group) in right_groups.iter().enumerate() {
            if !visible_groups.contains(group) {
                continue;
            }

            elements.push(self.render_group(
                *group,
                content_height,
                palette,
                false,
                t,
                window,
                lumino_extras::i18n::Language::ZhCn,
                // perf 仅在 Dashboard 分组使用，右侧分组不需要，占位即可
                &ToolbarPerfContext {
                    playback_tick: 0.0,
                    ppq: 480,
                    tempo_points: &[],
                },
                arrangement_mode,
                visible_tools,
            ));

            if i < right_groups.len() - 1 {
                elements.push(space().width(group.spacing_after()).into());
            }
        }
    }

    /// 根据分组标识渲染对应工具栏区域
    #[allow(clippy::too_many_arguments)]
    fn render_group<'a>(
        &'a self,
        group: ToolbarGroup,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        has_selection: bool,
        t: &'static lumino_extras::i18n::MainTranslations,
        window: &'a window::Window,
        language: lumino_extras::i18n::Language,
        perf: &ToolbarPerfContext<'_>,
        arrangement_mode: bool,
        visible_tools: &[ToolItem],
    ) -> Element<'a> {
        match group {
            ToolbarGroup::Record => {
                self.render_record_button(content_height, palette, window, language)
            }
            ToolbarGroup::Playback => {
                self.render_playback_controls(content_height, palette, t, window)
            }
            ToolbarGroup::Loop => self.render_loop_button(content_height, palette, t, window),
            ToolbarGroup::UndoRedo => {
                self.render_undo_redo_controls(content_height, palette, t, window)
            }
            ToolbarGroup::Dashboard => self.render_detection_dashboard(
                content_height,
                palette,
                perf.playback_tick,
                perf.ppq,
                perf.tempo_points,
            ),
            ToolbarGroup::Tools => self.render_tools_section(
                content_height,
                palette,
                has_selection,
                t,
                window,
                language,
                arrangement_mode,
                visible_tools,
            ),
            ToolbarGroup::OnionSkin => {
                self.render_onion_skin_button(content_height, palette, t, window)
            }
            ToolbarGroup::AutoScroll => {
                self.render_auto_scroll_button(content_height, palette, t, window)
            }
            ToolbarGroup::Collaboration => {
                self.render_collaboration_button(content_height, palette, t, window)
            }
        }
    }

    /// 渲染“更多”按钮（触发溢出菜单）
    ///
    /// ⚠️ 宽度**固定**为 [`MORE_BUTTON_WIDTH`]（= 普通工具按钮宽）：需求是
    /// "更多按钮永远保持大小不变、不被裁掉"，所以它既不吃 `Fill`、也不参与压缩，
    /// 由 `compute_overflow` 在预算里预先留好它的位置。
    fn render_more_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static lumino_extras::i18n::MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        use crate::resources::icon;
        use crate::toolbar::Event;
        use crate::toolbar::buttons::tool_button;

        container(tool_button(
            icon::EllipsisVertical,
            t.toolbar_more,
            Event::toggle_overflow_menu(),
            window,
            Some(Event::button_hovered(Some(ButtonId::More))),
        ))
        .width(Length::Fixed(MORE_BUTTON_WIDTH))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        })
        .into()
    }
}
