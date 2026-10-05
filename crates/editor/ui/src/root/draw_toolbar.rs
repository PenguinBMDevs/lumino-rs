//! 音符绘制悬浮工具条（覆盖层，非阻塞）
//!
//! 把原本内嵌在工具栏「绘制入口」右侧小三角里的绘制工具集，抽成一条**独立悬浮工具条**：
//! 以 `Stack` 顶层叠加于钢琴卷帘之上，默认停在卷帘区域下方、水平居中，可拖拽。
//! 外形为左右两端全圆的胶囊形（圆角 = 面板高度 / 2），配色随主题。
//!
//! 面板常驻显示全部绘制工具（曲线 / 颜料桶 / 画刷 / 形状 / 文字），
//! 当前激活工具高亮；颜料桶为可切换的「填充开关」（可与曲线/形状共存高亮）。
//! 开关由工具栏「绘制入口」按钮（`ToggleToolPanel`）控制，状态存于
//! `Toolbar::tool_panel_open`，拖拽偏移存于 `Toolbar::tool_panel_offset`。
//!
//! 拖拽：由胶囊左端的**专用拖拽柄**（`mouse_area` + 抓取纹样）`on_press` 起拖；
//! 松手（`ToolPanelDragEnded`）时若释放点接近默认位则**自动吸附回默认位**。
//!
//! ⚠️ 为什么必须单独做拖拽柄而不能把拖拽挂到整条胶囊上：iced 的 `button` 在
//! `ButtonPressed` 时会 `shell.capture_event()`（见 `iced_widget::button::update`），
//! 而胶囊主体几乎全被图标按钮占满 —— 按压会被按钮吞掉，外层 `mouse_area` 的
//! `on_press` 根本收不到，表现为"拖不动"。拖拽柄独立于按钮区，保证起拖信号必达。
//!
//! 起拖后叠加**全窗口透明覆盖层**接管 `on_move` / `on_release`（与
//! `root/mixer_panel.rs` 同一套机制），使光标离开面板/窗口范围时仍持续跟随。
//!
//! Ctrl+点击条目：打开该工具的「设置」（画刷设置 / 形状选择 / 分音符填充）——
//! 即旧主工具栏入口按钮 Ctrl 行为的整体迁移；设置下拉复用 `CurveToolGroup` 悬浮层，
//! 锚定在胶囊**上方**（胶囊贴近窗口底部时自动上翻）。

use iced_core::alignment::{Horizontal, Vertical};
use iced_core::{Background, Border, Color, Length, Padding};
use iced_widget::{Space, Stack, button, container, mouse_area, row, text, tooltip};

use crate::message::FillDivisionAction;
use crate::resources::icon;
use crate::root::Root;
use crate::toolbar::{CurveToolGroup, Event, ShapeType, Tool, ToolPanelItem};
use crate::{Element, Message, Theme};
use lumino_extras::i18n::main_translations;

/// 单个图标按钮尺寸（宽高相同）
const BUTTON_SIZE: f32 = 34.0;
/// 图标内部大小
const ICON_SIZE: u32 = 22;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距（胶囊高度 = BUTTON_SIZE + 2 × PANEL_PADDING）
const PANEL_PADDING: f32 = 5.0;
/// 胶囊圆角半径 = 面板高度 / 2（左右两端全圆）
const PANEL_RADIUS: f32 = BUTTON_SIZE / 2.0 + PANEL_PADDING;
/// 拖拽柄抓取区域宽度
const GRIP_WIDTH: f32 = 22.0;
/// 拖拽柄竖条宽度
const GRIP_BAR_WIDTH: f32 = 2.0;
/// 拖拽柄竖条高度
const GRIP_BAR_HEIGHT: f32 = 12.0;
/// 拖拽柄两根竖条之间的间距
const GRIP_BAR_GAP: f32 = 3.0;
/// 工具设置下拉（画刷 / 形状）的宽度
const MENU_WIDTH: f32 = 248.0;

impl Root {
    /// 渲染「音符绘制悬浮工具条」（未打开或非钢琴卷帘视图时返回 `None`）
    pub(crate) fn view_draw_toolbar(&self) -> Option<Element<'_>> {
        if !self.toolbar.tool_panel_open {
            return None;
        }
        // 仅在钢琴卷帘编辑区可见时渲染——与右侧栏同一"可见性口径"，避免在
        // 工程走带 / 瀑布流 / 音频视频导出面板残留悬浮条。
        if !self.right_sidebar_visible() {
            return None;
        }

        let t = main_translations(self.settings.display.language);
        let cur = self.toolbar.current_tool;
        let fill = self.toolbar.fill_enabled;
        let ctrl = self.toolbar.ctrl_pressed;

        // 面板条目：全部绘制工具**常显**（当前激活项高亮），不再做旧下拉的"隐藏当前工具"去重。
        // 第三项为 tooltip 所用的简短名称。
        let items: &[(ToolPanelItem, icon::Icon, &'static str, bool)] = &[
            (
                ToolPanelItem::Curve,
                icon::Curve,
                t.tool_curve,
                cur == Tool::Curve,
            ),
            (
                ToolPanelItem::FillBucket,
                icon::PaintBucket,
                t.tool_fill,
                fill,
            ),
            (
                ToolPanelItem::Brush,
                icon::BrushTool,
                t.tool_brush,
                cur == Tool::Brush,
            ),
            (
                ToolPanelItem::Shape,
                shape_icon(self.toolbar.current_shape),
                t.tool_shape,
                cur == Tool::Shape,
            ),
            (
                ToolPanelItem::Text,
                icon::TextInput,
                t.tool_text,
                cur == Tool::Text,
            ),
        ];

        // 首项为**专用拖拽柄**（独立于按钮区，保证起拖信号不被按钮吞掉），
        // 其后为分隔竖线，再是全部绘制工具图标（常显 + 激活高亮）。
        let mut row_items: Vec<Element<'static>> = Vec::with_capacity(items.len() + 2);
        row_items.push(drag_handle());
        row_items.push(grip_divider());
        row_items.extend(items.iter().map(|(item, ic, desc, selected)| {
            // Ctrl+点击且该条目有独立设置 → 「选择 + 打开设置」；否则普通选择。
            let on_press = if ctrl && has_settings(*item) {
                Event::tool_panel_item_ctrl_selected(*item)
            } else {
                Event::tool_panel_item_selected(*item)
            };
            tool_button(*ic, desc, *selected, on_press, &self.window.theme)
        }));

        // 胶囊面板：横向图标栏 + 主题配色 + 全圆角背景。
        let pill = container(
            row(row_items)
                .spacing(BUTTON_SPACING)
                .align_y(Vertical::Center),
        )
        .padding(PANEL_PADDING)
        .style(panel_style);

        // 胶囊本体仍追加一层 mouse_area（覆盖内边距环）：点头部内边距空白亦可靠开始拖拽；
        // 图标按钮区与拖拽柄为更内层，按下时由它们优先捕获，不会误触此层。
        // 同时挂 on_release 结束拖拽：快速点击（按下即抬起、全窗口覆盖层尚未挂载）时兜底，
        // 避免 dragging 残留导致拖拽覆盖层卡住。
        let pill_el: Element<'static> = mouse_area(pill)
            .on_press(Event::tool_panel_drag_started())
            .on_release(Event::tool_panel_drag_ended())
            .into();

        // 工具设置下拉（画刷 / 形状 / 颜料桶分音符填充）：复用 CurveToolGroup 悬浮层，
        // 锚定在胶囊**上方**（水平居中）。点击下拉内空白即关闭（mouse_area 包裹），
        // 下拉内按钮 / 输入框仍优先响应自身事件。
        let content: Element<'_> = match self.draw_tool_settings_menu() {
            Some((menu, close_message)) => {
                let panel_with_close: Element<'_> = mouse_area(menu).on_press(close_message).into();
                CurveToolGroup::new(pill_el, Some(panel_with_close), MENU_WIDTH).into()
            }
            None => pill_el,
        };

        // 定位：水平居中（dx = 相对中心的偏移），底部对齐 + dy 内缩。
        // 用左右不等的 padding 制造中心平移：居中时平移量 = (left - right) / 2，
        // 故取 left / right = 2 × |dx| 即得所需位移 dx。
        let (dx, dy) = self.toolbar.tool_panel_offset;
        let centered = container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Bottom)
            .padding(Padding {
                top: 0.0,
                bottom: dy,
                left: dx.max(0.0) * 2.0,
                right: (-dx).max(0.0) * 2.0,
            });

        // 拖拽进行中：叠加全窗口透明覆盖层，接管 on_move / on_release，实现"始终跟随鼠标"。
        if self.toolbar.tool_panel_dragging {
            let drag_overlay = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                .on_move(|p| Event::tool_panel_dragged(p.x, p.y))
                .on_release(Event::tool_panel_drag_ended());
            return Some(Stack::new().push(centered).push(drag_overlay).into());
        }

        Some(centered.into())
    }

    /// 构建悬浮条的「工具设置」下拉（画刷 / 形状 / 颜料桶）。
    ///
    /// 返回 `(菜单元素, 点击菜单外空白时的关闭消息)`；仅当对应下拉处于打开态时
    /// 返回 `Some`（三者互斥）。面板配色贴近工具栏（工具栏底色压暗 10%），
    /// 与旧主工具栏入口按钮下拉保持一致观感。
    ///
    /// 颜料桶的「分音符填充」面板由 `state.fill_division_dialog.is_open` 驱动
    /// （画布 Ctrl+单击与悬浮条 Ctrl+单击颜料桶两条路径共用同一状态），
    /// 渲染为与画刷 / 形状同风格的小面板——不再是全屏居中弹窗。
    fn draw_tool_settings_menu(&self) -> Option<(Element<'_>, Message)> {
        let palette = self.window.theme.extended_palette();
        let toolbar_bg = palette.background.weakest.color;
        let panel_background = Color::from_rgba(
            toolbar_bg.r * 0.9,
            toolbar_bg.g * 0.9,
            toolbar_bg.b * 0.9,
            toolbar_bg.a,
        );

        if self.toolbar.brush_dropdown_open {
            let menu: Element<'_> =
                container(crate::toolbar::brush_dropdown::render_brush_dropdown(
                    &self.toolbar.brush,
                    self.settings.display.language,
                    panel_background,
                    &self.window.theme,
                ))
                .width(Length::Fixed(MENU_WIDTH))
                .height(Length::Shrink)
                .into();
            Some((menu, Event::close_brush_dropdown()))
        } else if self.toolbar.shape_dropdown_open {
            let menu: Element<'_> =
                container(crate::toolbar::shape_dropdown::render_shape_dropdown(
                    self.toolbar.current_shape,
                    panel_background,
                    &self.window.theme,
                ))
                .width(Length::Fixed(MENU_WIDTH))
                .height(Length::Shrink)
                .into();
            Some((menu, Event::close_shape_dropdown()))
        } else if self.state.fill_division_dialog.is_open {
            let menu: Element<'_> = container(
                crate::toolbar::fill_division_dropdown::render_fill_division_dropdown(
                    &self.state.fill_division_dialog.value,
                    self.settings.display.language,
                    panel_background,
                    &self.window.theme,
                ),
            )
            .width(Length::Fixed(MENU_WIDTH))
            .height(Length::Shrink)
            .into();
            Some((menu, Message::FillDivision(FillDivisionAction::CloseDialog)))
        } else {
            None
        }
    }
}

/// 该条目是否有可打开的「工具设置」（Ctrl+点击触发）。
///
/// 画刷 → 画刷设置下拉；形状 → 形状选择下拉；颜料桶 → 分音符填充对话框。
/// 曲线 / 文字无独立设置，Ctrl+点击退化为普通选择。
fn has_settings(item: ToolPanelItem) -> bool {
    matches!(
        item,
        ToolPanelItem::Brush | ToolPanelItem::Shape | ToolPanelItem::FillBucket
    )
}

/// 胶囊左端的**专用拖拽柄**：两根竖细条 + 独立 `mouse_area`（按下即起拖）。
///
/// 必须独立于图标按钮区 —— iced 的 `button` 在按下时会捕获事件，若把拖拽挂在
/// 整条胶囊上，按压会被按钮吞掉、收不到 `on_press`（即"拖不动"的根因）。
/// 此区域不含任何按钮，按下必达 `ToolPanelDragStarted`。
fn drag_handle() -> Element<'static> {
    let bar = |height: f32| -> Element<'static> {
        container(
            Space::new()
                .width(Length::Fixed(GRIP_BAR_WIDTH))
                .height(Length::Fixed(height)),
        )
        .style(grip_bar_style)
        .into()
    };

    let visual = container(
        row(vec![bar(GRIP_BAR_HEIGHT), bar(GRIP_BAR_HEIGHT)])
            .spacing(GRIP_BAR_GAP)
            .align_y(Vertical::Center),
    )
    .width(Length::Fixed(GRIP_WIDTH))
    .height(Length::Fixed(BUTTON_SIZE))
    .align_x(Horizontal::Center)
    .align_y(Vertical::Center);

    mouse_area(visual)
        .on_press(Event::tool_panel_drag_started())
        .on_release(Event::tool_panel_drag_ended())
        .into()
}

/// 拖拽柄与工具图标区之间的分隔竖线，提示"左端为拖拽把手"。
fn grip_divider() -> Element<'static> {
    container(
        Space::new()
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(BUTTON_SIZE - 12.0)),
    )
    .style(divider_style)
    .into()
}

/// 拖拽柄竖条样式：取主题最强背景色（深色主题下为浅灰，与胶囊底色形成对比）。
fn grip_bar_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.strongest.color)),
        border: Border::default().rounded(1.0),
        ..Default::default()
    }
}

/// 分隔竖线样式：较弱一档，仅作视觉区隔。
fn divider_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.strong.color)),
        ..Default::default()
    }
}

/// 形状工具图标随当前图形类型切换（矩形 / 圆形 / 三角形）
fn shape_icon(shape: ShapeType) -> icon::Icon {
    match shape {
        ShapeType::Rectangle => icon::ShapeRectangle,
        ShapeType::Circle => icon::ShapeCircle,
        ShapeType::Triangle => icon::ShapeTriangle,
    }
}

/// 构建面板中的图标独占按钮（图标 + 悬浮 tooltip，避免把文字塞进按钮撑宽）
///
/// `on_press` 由调用方按 Ctrl 状态决定：普通选择或「选择 + 打开设置」。
fn tool_button(
    ic: icon::Icon,
    desc: &'static str,
    selected: bool,
    on_press: Message,
    theme: &Theme,
) -> Element<'static> {
    let icon_el = icon::view_with_size_and_theme(ic, ICON_SIZE, ICON_SIZE, Some(theme));
    let btn = button(icon_el)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(on_press)
        .style(move |theme: &Theme, status| button_style(theme, status, selected));

    tooltip::Tooltip::new(btn, text(desc), tooltip::Position::Top)
        .style(tooltip_style)
        .into()
}

/// 图标按钮样式：选中态用主题强调色，悬停 / 按下用更亮一档，常态透明。
fn button_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let p = theme.extended_palette();
    let background = if selected {
        p.background.strong.color
    } else {
        match status {
            button::Status::Hovered => p.background.stronger.color,
            button::Status::Pressed => p.background.strongest.color,
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        border: Border::default().rounded(BUTTON_SIZE / 2.0),
        ..Default::default()
    }
    .with_background(background)
}

/// 胶囊面板背景：随主题（弱背景 + 强调色描边），圆角 = 高度 / 2。
fn panel_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.weak.color)),
        border: Border {
            color: p.background.strongest.color,
            width: 1.0,
            radius: PANEL_RADIUS.into(),
        },
        ..Default::default()
    }
}

/// Tooltip 样式：随主题（强背景 + 弱前景文字）
fn tooltip_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.strongest.color)),
        border: Border::default().rounded(4),
        text_color: Some(p.background.weakest.text),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    /// 渲染冒烟测试：悬浮工具条在两种开关状态下构建/开关均不 panic。
    #[test]
    fn test_view_draw_toolbar_closed_returns_none() {
        let root = Root::new(&UiConfig::default());
        assert!(
            root.view_draw_toolbar().is_none(),
            "工具条未打开时应返回 None"
        );
    }

    #[test]
    fn test_view_draw_toolbar_open_builds() {
        let mut root = Root::new(&UiConfig::default());
        root.toolbar.tool_panel_open = true;
        let _element = root.view_draw_toolbar();
    }

    #[test]
    fn test_shape_icon_reflects_kind() {
        assert!(matches!(
            shape_icon(ShapeType::Rectangle),
            icon::Icon::ShapeRectangle
        ));
        assert!(matches!(
            shape_icon(ShapeType::Circle),
            icon::Icon::ShapeCircle
        ));
        assert!(matches!(
            shape_icon(ShapeType::Triangle),
            icon::Icon::ShapeTriangle
        ));
    }

    /// 设置判定：仅画刷 / 形状 / 颜料桶有独立设置，其余退化为普通选择。
    #[test]
    fn test_has_settings_scope() {
        assert!(has_settings(ToolPanelItem::Brush));
        assert!(has_settings(ToolPanelItem::Shape));
        assert!(has_settings(ToolPanelItem::FillBucket));
        assert!(!has_settings(ToolPanelItem::Curve));
        assert!(!has_settings(ToolPanelItem::Text));
        assert!(!has_settings(ToolPanelItem::StrokeSettings));
    }
}
