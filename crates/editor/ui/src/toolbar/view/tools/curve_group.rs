use super::*;

use iced_core::{Color, Length};
use iced_widget::mouse_area;

use crate::Message;
use crate::toolbar::buttons::{tool_dropdown_caret, tool_selector_custom};
use crate::toolbar::view::curve_tool_group::CurveToolGroup;
use crate::toolbar::{brush_dropdown, shape_dropdown, tool_panel};

impl Toolbar {
    /// 渲染「曲线工具组」：曲线工具按钮（图标随激活子工具切换）+ 右侧小三角
    ///
    /// 小三角展开「绘制工具选择面板」（合并后的工具集）。下拉菜单锚定在按钮正下方，
    /// 点击面板外部区域由全窗口遮罩层关闭。普通点击曲线按钮 = 选择曲线工具（基础态），
    /// Ctrl+点击 = 打开画刷工具下拉。
    pub(super) fn render_curve_tool_group<'a>(
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
}
