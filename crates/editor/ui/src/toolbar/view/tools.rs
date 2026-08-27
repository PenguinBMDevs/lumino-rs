//! 工具栏工具选择区域渲染
//!
//! 包含钢琴卷帘工具区（指针/铅笔/橡皮/曲线/颜料桶 + 量化/变速/翻转/
//! 分割/合并/移调/连奏/精度）与工程走带工具区（指针/曲线/橡皮/变速）。

use iced_core::{Alignment, Color, Length};
use iced_widget::{container, mouse_area, row, space};

use crate::resources::icon;
use crate::toolbar::buttons::{
    flip_button, tool_button, tool_dropdown_caret, tool_selector, tool_selector_custom,
};
use crate::toolbar::view::curve_tool_group::CurveToolGroup;
use crate::toolbar::{
    ButtonId, Event, FlipHorizontalMode, Tool, Toolbar, brush_dropdown, shape_dropdown, tool_panel,
};
use crate::{Element, Message, Theme, window};
use lumino_extras::i18n::{Language, MainTranslations};

impl Toolbar {
    /// 渲染工具选择区域（指针/铅笔/橡皮/曲线/颜料桶 + 量化/变速/翻转/分割/合并/移调 + 精度下拉），宽度自适应
    #[allow(clippy::too_many_arguments)]
    pub fn render_tools_section<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
        language: Language,
        arrangement_mode: bool,
    ) -> Element<'a> {
        if arrangement_mode {
            return self.render_arrangement_tools_section(
                content_height,
                palette,
                has_selection,
                t,
                window,
            );
        }

        let (transpose_down_tooltip, transpose_down_event) = if self.ctrl_pressed {
            (t.tool_transpose_down_octave, Event::transpose_down(12))
        } else {
            (t.tool_transpose_down, Event::transpose_down(1))
        };
        let (transpose_up_tooltip, transpose_up_event) = if self.ctrl_pressed {
            (t.tool_transpose_up_octave, Event::transpose_up(12))
        } else {
            (t.tool_transpose_up, Event::transpose_up(1))
        };

        container(
            row![
                tool_selector(
                    icon::MousePointer,
                    t.tool_pointer,
                    Tool::Pointer,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pointer))),
                ),
                space().width(4),
                tool_selector(
                    icon::MousePointerYSelect,
                    t.tool_pointer_y_select,
                    Tool::PointerYSelect,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::PointerYSelect))),
                ),
                space().width(4),
                tool_selector(
                    icon::Pencil,
                    t.tool_pencil,
                    Tool::Pencil,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pencil))),
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    t.tool_eraser,
                    Tool::Eraser,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Eraser))),
                ),
                space().width(4),
                // 曲线工具组：曲线工具按钮 + 右侧小三角（合并后的绘制工具集入口）。
                // - 曲线工具按钮图标随当前激活的绘制子工具切换（画刷/形状/文字激活时显示对应图标，
                //   填充开启时显示颜料桶）；其选中高亮与工具栏其他工具按钮保持一致。
                // - 小三角展开「绘制工具选择面板」（填充桶/画刷/形状/文字/橡皮擦）。
                // - 下拉菜单锚定在按钮正下方，点击面板外部区域关闭。
                self.render_curve_tool_group(t, window, language),
                space().width(4),
                tool_button(
                    icon::Quantize,
                    t.tool_quantize,
                    Event::quantize(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Quantize))),
                ),
                space().width(4),
                // 变速按钮始终可点击：Ctrl+Click 打开变速对话框不需要选中音符。
                // 普通点击的无选中情况由 handler 内部的 selected.is_empty() 兜底。
                flip_button(
                    icon::Speed,
                    t.tool_speed,
                    Event::speed_change(),
                    true,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Speed))),
                ),
                space().width(4),
                flip_button(
                    icon::FlipVertical,
                    t.tool_flip_vertical,
                    Event::flip_vertical(),
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::FlipVertical))),
                ),
                space().width(4),
                flip_button(
                    icon::FlipHorizontal,
                    t.tool_flip_horizontal,
                    if self.shift_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Right)
                    } else if self.ctrl_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Left)
                    } else {
                        Event::flip_horizontal(FlipHorizontalMode::Center)
                    },
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::FlipHorizontal))),
                ),
                space().width(8),
                // 分割/合并按钮
                tool_button(
                    icon::Split,
                    t.tool_split,
                    Event::split(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Split))),
                ),
                space().width(4),
                tool_button(
                    icon::Glue,
                    t.tool_glue,
                    Event::glue(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Glue))),
                ),
                space().width(8),
                // 移调按钮
                // 普通点击 ±1 半音，Ctrl+点击 ±12 半音（一个八度）
                flip_button(
                    icon::TransposeDown,
                    transpose_down_tooltip,
                    transpose_down_event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeDown))),
                ),
                space().width(4),
                flip_button(
                    icon::TransposeUp,
                    transpose_up_tooltip,
                    transpose_up_event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeUp))),
                ),
                space().width(8),
                // 连奏按钮
                tool_button(
                    icon::Tie,
                    t.tool_tie,
                    Event::tie(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Tie))),
                ),
                space().width(8),
                self.render_precision_selector(content_height, palette, language, t),
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Shrink)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        })
        .into()
    }

    /// 渲染「曲线工具组」：曲线工具按钮（图标随激活子工具切换）+ 右侧小三角
    ///
    /// 小三角展开「绘制工具选择面板」（合并后的工具集）。下拉菜单锚定在按钮正下方，
    /// 点击面板外部区域由全窗口遮罩层关闭。普通点击曲线按钮 = 选择曲线工具（基础态），
    /// Ctrl+点击 = 打开画刷工具下拉。
    fn render_curve_tool_group<'a>(
        &'a self,
        t: &'static MainTranslations,
        window: &'a window::Window,
        language: Language,
    ) -> Element<'a> {
        // 曲线工具按钮图标：随当前激活的绘制子工具切换
        let curve_icon = match self.current_tool {
            Tool::Curve if self.fill_enabled => icon::PaintBucket,
            Tool::Brush => icon::BrushTool,
            // 形状工具激活时，图标反映当前选中的图形类型（矩形/圆形/三角形），
            // 让用户一眼看到正在绘制的图形；Ctrl+点击弹出图形选择下拉。
            Tool::Shape => match self.current_shape {
                crate::toolbar::ShapeType::Rectangle => icon::ShapeRectangle,
                crate::toolbar::ShapeType::Circle => icon::ShapeCircle,
                crate::toolbar::ShapeType::Triangle => icon::ShapeTriangle,
            },
            Tool::Text => icon::TextInput,
            _ => icon::Curve,
        };
        // 选中高亮：当前处于绘制家族工具之一（曲线/画刷/形状/文字）；
        // 橡皮擦有独立按钮，故不在此高亮，避免双高亮。
        let curve_selected = matches!(
            self.current_tool,
            Tool::Curve | Tool::Brush | Tool::Shape | Tool::Text
        );
        // 普通点击 = 选择曲线工具（基础态）；仅当当前已处于画刷工具时，
        // Ctrl+点击才打开画刷工具下拉（设置面板）。非画刷工具下 Ctrl+点击应
        // 退化为普通点击（选择曲线工具），不应误弹画刷设置面板。
        // 决策逻辑抽出到 `Toolbar::curve_button_press_event` 以便回归测试。
        let curve_on_press = Message::Toolbar(self.curve_button_press_event());
        let curve_btn = tool_selector_custom(
            curve_icon,
            t.tool_curve,
            curve_selected,
            curve_on_press,
            window,
            Some(Event::button_hovered(Some(ButtonId::Curve))),
        );

        // 右侧小三角：展开「绘制工具选择面板」
        let caret_btn = tool_dropdown_caret(
            icon::ToolPanelCaret,
            t.tool_panel_tooltip,
            Event::toggle_tool_panel(),
            window,
            Some(Event::button_hovered(Some(ButtonId::ToolPanel))),
        );

        // 面板背景色：贴近工具栏背景
        let palette = window.theme.extended_palette();
        let toolbar_bg = palette.background.weakest.color;
        let panel_background = Color::from_rgba(
            toolbar_bg.r * 0.9,
            toolbar_bg.g * 0.9,
            toolbar_bg.b * 0.9,
            toolbar_bg.a,
        );

        // 下拉菜单宽度（像素），用于约束 overlay 布局。
        // 面板改为「图标独占横向排列」后，内容宽约 5×40 + 4×4(间距) + 2×8(内边距) = 232px，
        // 这里取 248 留少量余量，避免裁切。
        let menu_width = 248.0;

        // 下拉菜单：绘制工具选择面板（填充桶/画刷/形状/文字/橡皮擦）、画刷工具下拉，
        // 或形状工具下拉（矩形/圆形/三角形）。三者互斥，仅其一打开。菜单锚定在按钮
        // 正下方，点击外部由 overlay 关闭。
        let menu: Option<Element<'a>> = if self.tool_panel_open {
            Some(
                container(tool_panel::render_tool_panel(
                    self.current_tool,
                    self.fill_enabled,
                    language,
                    panel_background,
                    &window.theme,
                ))
                .width(Length::Fixed(menu_width))
                .height(Length::Shrink)
                .into(),
            )
        } else if self.brush_dropdown_open {
            Some(
                container(brush_dropdown::render_brush_dropdown(
                    &self.brush,
                    language,
                    panel_background,
                    &window.theme,
                ))
                .width(Length::Fixed(menu_width))
                .height(Length::Shrink)
                .into(),
            )
        } else if self.shape_dropdown_open {
            Some(
                container(shape_dropdown::render_shape_dropdown(
                    self.current_shape,
                    panel_background,
                    &window.theme,
                ))
                .width(Length::Fixed(menu_width))
                .height(Length::Shrink)
                .into(),
            )
        } else {
            None
        };

        // 点击菜单外部区域时发布的关闭消息（与当前打开的下拉对应）
        let close_message = if self.tool_panel_open {
            Event::close_tool_panel()
        } else if self.brush_dropdown_open {
            Event::close_brush_dropdown()
        } else {
            Event::close_shape_dropdown()
        };

        // 垂直居中对齐，使右侧小三角与曲线按钮在同一中轴线上（否则小三角会贴顶）。
        let content = row![curve_btn, space().width(2), caret_btn]
            .align_y(Alignment::Center)
            .into();

        match menu {
            Some(panel) => {
                // 面板背景用 mouse_area 包裹：点击面板内空白即关闭下拉；
                // 面板内按钮仍优先响应自身 on_press（与右键悬浮面板 context_menu 同源，
                // mouse_area 不会吞掉子按钮点击）。面板整体作为 CurveToolGroup 的
                // overlay，由 iced 标准 Overlay 机制锚定在按钮正下方并转发点击——
                // 这正是此前"按钮点不动 / 高度裁切"病灶的根除方案。
                let panel_with_close = mouse_area(panel).on_press(close_message).into();
                CurveToolGroup::new(content, Some(panel_with_close), menu_width).into()
            }
            None => content,
        }
    }

    /// 渲染工程走带视图专用的工具选择区域
    ///
    /// 仅开启 yinhe 工程走带面板支持的工具：选择/铅笔/曲线/切割/橡皮擦。
    fn render_arrangement_tools_section<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        _has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(
            row![
                tool_selector(
                    icon::MousePointer,
                    t.tool_pointer,
                    Tool::Pointer,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pointer))),
                ),
                space().width(4),
                tool_selector(
                    icon::Curve,
                    t.tool_curve,
                    Tool::Curve,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Curve))),
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    t.tool_eraser,
                    Tool::Eraser,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Eraser))),
                ),
                space().width(4),
                flip_button(
                    icon::Speed,
                    t.tool_speed,
                    Event::speed_change(),
                    true,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Speed))),
                ),
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Shrink)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        })
        .into()
    }
}
