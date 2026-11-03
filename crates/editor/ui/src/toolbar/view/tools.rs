//! 工具栏工具选择区域渲染
//!
//! 包含钢琴卷帘工具区（指针/铅笔/橡皮/曲线/颜料桶 + 量化/变速/翻转/
//! 分割/合并/移调/连奏/精度）与工程走带工具区（指针/曲线/橡皮/变速）。
//!
//! # 逐项折叠
//!
//! 工具区是**唯一**支持"逐项折叠"的分组：宽度不够时按
//! [`ToolItem`] 的顺序从右往左一个个收起（见 `overflow::interaction`），
//! 本模块只负责渲染"当前还看得见的那些条目"。
//!
//! 顺序与间距的**唯一来源**是 [`ToolItem::ALL`] / [`ToolItem::gap_before`]：
//! 渲染循环读它、宽度预算也读它，两边不会漂移。

mod arrangement;
mod curve_group;

use iced_core::{Alignment, Length};
use iced_widget::{Row, container, row, space};

use crate::resources::icon;
use crate::toolbar::buttons::{flip_button, tool_button, tool_selector};
use crate::toolbar::overflow::ToolItem;
use crate::toolbar::overflow::interaction::PRECISION_GAP;
use crate::toolbar::{ButtonId, Event, FlipHorizontalMode, Tool, Toolbar};
use crate::{Element, Theme, window};
use lumino_extras::i18n::{Language, MainTranslations};

impl Toolbar {
    /// 渲染工具选择区域（仅渲染 `visible_items` 中的条目 + 精度选择器）
    ///
    /// `visible_items` 必须是 [`ToolItem::ALL`] 的**前缀**（折叠只从右端剥离），
    /// 由 `compute_overflow` 保证；这里不再做二次校验。
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
        visible_items: &[ToolItem],
    ) -> Element<'a> {
        if arrangement_mode {
            // 走带模式的工具区固定四枚，整组折叠（不做逐项）
            return self.render_arrangement_tools_section(
                content_height,
                palette,
                has_selection,
                t,
                window,
            );
        }

        let mut items: Vec<Element<'a>> = Vec::with_capacity(visible_items.len() * 2 + 2);
        for (index, item) in visible_items.iter().copied().enumerate() {
            if index > 0 {
                items.push(space().width(item.gap_before()).into());
            }
            items.push(self.render_tool_item(item, has_selection, t, window, language));
        }
        // 精度选择器随组常驻：它不是"工具按钮"，不参与逐项折叠
        if !visible_items.is_empty() {
            items.push(space().width(PRECISION_GAP).into());
        }
        items.push(self.render_precision_selector(content_height, palette, language, t));

        container(Row::with_children(items).align_y(Alignment::Center))
            .width(Length::Shrink)
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

    /// 渲染主工具栏中的**单个**条目
    ///
    /// 与逐项折叠前的整排渲染逐项等价：图标、tooltip、事件、禁用条件都保持不变，
    /// 唯一区别是"按条目返回"，便于只画可见的那几个。
    fn render_tool_item<'a>(
        &'a self,
        item: ToolItem,
        has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
        language: Language,
    ) -> Element<'a> {
        let help_tip = |tool: Tool, fallback: &'static str| -> &'a str {
            lumino_ui_core::help::tool_tip(tool, language).unwrap_or(fallback)
        };

        match item {
            ToolItem::Pointer => tool_selector(
                icon::MousePointer,
                help_tip(Tool::Pointer, t.tool_pointer),
                Tool::Pointer,
                self.current_tool,
                window,
                Some(Event::button_hovered(Some(ButtonId::Pointer))),
            ),
            ToolItem::PointerYSelect => tool_selector(
                icon::MousePointerYSelect,
                help_tip(Tool::PointerYSelect, t.tool_pointer_y_select),
                Tool::PointerYSelect,
                self.current_tool,
                window,
                Some(Event::button_hovered(Some(ButtonId::PointerYSelect))),
            ),
            ToolItem::Pencil => tool_selector(
                icon::Pencil,
                help_tip(Tool::Pencil, t.tool_pencil),
                Tool::Pencil,
                self.current_tool,
                window,
                Some(Event::button_hovered(Some(ButtonId::Pencil))),
            ),
            ToolItem::Eraser => tool_selector(
                icon::Eraser,
                help_tip(Tool::Eraser, t.tool_eraser),
                Tool::Eraser,
                self.current_tool,
                window,
                Some(Event::button_hovered(Some(ButtonId::Eraser))),
            ),
            // 音符画工具箱入口开关：固定图标，仅随浮层开合高亮
            ToolItem::DrawToolbox => self.render_draw_tool_toggle(t, window),
            ToolItem::Quantize => tool_button(
                icon::Quantize,
                t.tool_quantize,
                Event::quantize(),
                window,
                Some(Event::button_hovered(Some(ButtonId::Quantize))),
            ),
            // 变速按钮始终可点击：Ctrl+Click 打开变速对话框不需要选中音符
            ToolItem::Speed => flip_button(
                icon::Speed,
                t.tool_speed,
                Event::speed_change(),
                true,
                window,
                Some(Event::button_hovered(Some(ButtonId::Speed))),
            ),
            ToolItem::FlipVertical => flip_button(
                icon::FlipVertical,
                t.tool_flip_vertical,
                Event::flip_vertical(),
                has_selection,
                window,
                Some(Event::button_hovered(Some(ButtonId::FlipVertical))),
            ),
            ToolItem::FlipHorizontal => flip_button(
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
            ToolItem::Split => tool_button(
                icon::Split,
                t.tool_split,
                Event::split(),
                window,
                Some(Event::button_hovered(Some(ButtonId::Split))),
            ),
            ToolItem::Glue => tool_button(
                icon::Glue,
                t.tool_glue,
                Event::glue(),
                window,
                Some(Event::button_hovered(Some(ButtonId::Glue))),
            ),
            // 普通点击 ±1 半音，Ctrl+点击 ±12 半音（一个八度）
            ToolItem::TransposeDown => {
                let (tooltip, event) = if self.ctrl_pressed {
                    (t.tool_transpose_down_octave, Event::transpose_down(12))
                } else {
                    (t.tool_transpose_down, Event::transpose_down(1))
                };
                flip_button(
                    icon::TransposeDown,
                    tooltip,
                    event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeDown))),
                )
            }
            ToolItem::TransposeUp => {
                let (tooltip, event) = if self.ctrl_pressed {
                    (t.tool_transpose_up_octave, Event::transpose_up(12))
                } else {
                    (t.tool_transpose_up, Event::transpose_up(1))
                };
                flip_button(
                    icon::TransposeUp,
                    tooltip,
                    event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeUp))),
                )
            }
            ToolItem::Tie => tool_button(
                icon::Tie,
                t.tool_tie,
                Event::tie(),
                window,
                Some(Event::button_hovered(Some(ButtonId::Tie))),
            ),
        }
    }
}
