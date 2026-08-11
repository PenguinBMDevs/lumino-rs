//! 工具栏工具选择区域渲染
//!
//! 包含钢琴卷帘工具区（指针/铅笔/橡皮/曲线/颜料桶 + 量化/变速/翻转/
//! 分割/合并/移调/连奏/精度）与工程走带工具区（指针/曲线/橡皮/变速）。

use iced_core::Alignment;
use iced_widget::{container, row, space};

use crate::resources::icon;
use crate::toolbar::buttons::{flip_button, toggle_button, tool_button, tool_selector};
use crate::toolbar::{ButtonId, Event, FlipHorizontalMode, Tool, Toolbar};
use crate::{Element, Theme, window};
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
                tool_selector(
                    icon::Curve,
                    t.tool_curve,
                    Tool::Curve,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Curve))),
                ),
                space().width(4),
                // 颜料桶（启用式开关）：仅曲线工具激活时可操作；
                // 选中高亮 = 填充模式开启，点击切换开/关
                toggle_button(
                    icon::PaintBucket,
                    t.tool_fill,
                    Event::fill_toggled(!self.fill_enabled),
                    self.current_tool == Tool::Curve,
                    self.fill_enabled,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Fill))),
                ),
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
