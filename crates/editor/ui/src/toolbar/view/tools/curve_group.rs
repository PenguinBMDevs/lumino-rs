use super::*;

use crate::toolbar::buttons::tool_selector_custom;

impl Toolbar {
    /// 渲染「音符画工具箱」开关按钮（固定样式，纯开关）。
    ///
    /// 该按钮由图集入口演化为**单一纯开关**：
    /// - **固定图标**：恒为 `icon::DrawToolbox`，不随当前激活的绘制子工具切换；
    /// - **固定选中态**：仅当「音符画悬浮工具条」打开时高亮，作为开关状态的唯一反馈；
    /// - 点击 = 开关悬浮工具条（面板本体由 `root/draw_toolbar.rs` 渲染，浮在卷帘区域上）。
    ///
    /// 注：绘制工具的**设置**（画刷 / 形状下拉、分音符填充）已随之迁到悬浮条，
    /// 由悬浮条条目的 Ctrl+点击触发（见 `root/draw_toolbar.rs`）；本按钮不再承载任何设置，
    /// Ctrl+点击与本按钮无关（即"完全搬到悬浮条"的迁移结果）。
    pub(super) fn render_draw_tool_toggle<'a>(
        &'a self,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        tool_selector_custom(
            icon::DrawToolbox,
            t.tool_panel_tooltip,
            self.tool_panel_open,
            Event::toggle_tool_panel(),
            window,
            Some(Event::button_hovered(Some(ButtonId::ToolPanel))),
        )
    }
}
