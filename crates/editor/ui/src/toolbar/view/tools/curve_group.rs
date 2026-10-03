use super::*;

use iced_core::{Color, Length};
use iced_widget::mouse_area;

use crate::Message;
use crate::toolbar::buttons::tool_selector_custom;
use crate::toolbar::view::curve_tool_group::CurveToolGroup;
use crate::toolbar::{brush_dropdown, shape_dropdown};

impl Toolbar {
    /// 渲染「音符画工具箱」开关按钮（固定样式）。
    ///
    /// 该按钮合并了原「曲线工具按钮 + 右侧小三角」两者，成为**单一开关**：
    /// - **固定图标**：恒为 `icon::DrawToolbox`，不再随当前激活的绘制子工具切换
    ///   （去掉旧的"图标随工具变形"行为，样式固定、稳定可预期）；
    /// - **固定选中态**：仅当「音符画悬浮工具条」打开时高亮，作为开关状态的唯一反馈；
    /// - 普通点击 = 开关悬浮工具条（面板本体由 `root/draw_toolbar.rs` 渲染，
    ///   浮在卷帘区域上），再次点击关闭；
    /// - Ctrl+点击仍保留旧旁路（见 `Toolbar::curve_button_press_event`）：画刷工具下拉、
    ///   形状工具下拉、分音符填充对话框。
    ///
    /// 注意：原「绘制工具选择面板」（`tool_panel`）下拉已迁出为独立悬浮工具条；
    /// 此处仅保留**画刷 / 形状的设置下拉**，仍由 `CurveToolGroup` 浮层锚定在按钮正下方。
    pub(super) fn render_draw_tool_toggle<'a>(
        &'a self,
        t: &'static MainTranslations,
        window: &'a window::Window,
        language: Language,
    ) -> Element<'a> {
        // 固定样式：入口按钮图标恒为「音符画工具箱」，不随当前激活的绘制子工具切换。
        let toggle_icon = icon::DrawToolbox;
        // 固定选中态：仅由悬浮工具条的开合状态决定，不再与「当前是否为绘制家族工具」耦合。
        let selected = self.tool_panel_open;

        let on_press = Message::Toolbar(self.curve_button_press_event());
        let toggle_btn = tool_selector_custom(
            toggle_icon,
            t.tool_panel_tooltip,
            selected,
            on_press,
            window,
            Some(Event::button_hovered(Some(ButtonId::ToolPanel))),
        );

        // 菜单：仅保留画刷 / 形状设置下拉（两者互斥，仅其一打开）。
        let palette = window.theme.extended_palette();
        let toolbar_bg = palette.background.weakest.color;
        let panel_background = Color::from_rgba(
            toolbar_bg.r * 0.9,
            toolbar_bg.g * 0.9,
            toolbar_bg.b * 0.9,
            toolbar_bg.a,
        );
        let menu_width = 248.0;

        let menu: Option<Element<'a>> = if self.brush_dropdown_open {
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
        let close_message = if self.brush_dropdown_open {
            Event::close_brush_dropdown()
        } else {
            Event::close_shape_dropdown()
        };

        match menu {
            Some(panel) => {
                // 面板背景用 mouse_area 包裹：点击面板内空白即关闭下拉；面板内按钮仍优先
                // 响应自身 on_press。面板作为 CurveToolGroup 的 overlay 锚定在按钮正下方。
                let panel_with_close = mouse_area(panel).on_press(close_message).into();
                CurveToolGroup::new(toggle_btn, Some(panel_with_close), menu_width).into()
            }
            None => toggle_btn,
        }
    }
}
