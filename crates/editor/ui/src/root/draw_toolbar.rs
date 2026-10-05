//! 音符绘制悬浮工具条（覆盖层，非阻塞）
//!
//! 把原本内嵌在工具栏「绘制入口」右侧小三角里的绘制工具集，抽成一条**独立悬浮工具条**：
//! 以 `Stack` 顶层叠加于钢琴卷帘之上，默认停在卷帘区域下方、水平居中，可拖拽。
//! 外形为左右两端全圆的胶囊形（圆角 = 面板高度 / 2），配色随主题。
//!
//! 面板常驻显示全部绘制工具（鼠标 / 曲线 / 颜料桶 / 画刷 / 形状 / 文字），
//! 当前激活工具高亮；颜料桶为可切换的「填充开关」（可与曲线/形状共存高亮）。
//! 胶囊**右端**另有一枚常显的**设置按钮**（齿轮），以分隔线与工具图标区隔：
//! 它是各绘制工具设置的**聚合入口**，**不参与工具选择语义**，且**不在主窗口落浮层**——
//! 点开的是**独立 OS 窗口**（`DialogType::DrawSettings`，见 `view/draw_settings_dialog.rs`），
//! 由 Runner 开窗并对同类型窗口去重。
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
//! 设置下拉复用 `CurveToolGroup` 悬浮层——**已启用条目再次点击**触发，
//! 锚定在触发它的图标按钮**上方**（胶囊贴近窗口顶部时自动下翻）。
//! （齿轮不在此列：它开的是独立窗口，主窗口内没有属于它的悬浮层。）
//!
//! **交互（现行）**：条目**已启用**时**再次点击同一条目**即弹出其设置面板，无需按 Ctrl
//! （点击→消息的唯一出口 = 本模块 `tool_panel_item_press`）。同一面板重复点击条目**保持
//! 打开**（不做开合 toggle，避免"双击习惯"把刚弹出的面板瞬间关掉）；关闭方式：
//! 点击面板内空白（`mouse_area(menu)` 的关闭消息）、点击胶囊内其它条目或拖拽柄
//! （外部事件先关下拉）、颜料桶面板的取消 / 关闭填充。
//!
//! **锚点**：设置面板挂在**触发它的那个图标按钮**上（`CurveToolGroup` 的锚点 = 其内容
//! 元素），因此水平居中于该按钮；若把整条胶囊作为锚点，面板会以"胶囊中心"居中——
//! 因胶囊（含右端齿轮约 329 宽）≈ 面板宽（248），视觉上就成了"贴在悬浮条左端"
//! （已修复的 BUG）。纵向上面板浮在锚点内容上方 2px（`PanelOverlay::layout`）：锚点由
//! 胶囊改为按钮后，面板底缘与胶囊顶缘有约 3px 重叠（按钮内缩在胶囊 5px 内边距里），
//! 呈现"从按钮上沿弹出"的观感；胶囊贴近窗口顶部时自动翻到按钮下方。

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

        // 面板条目：全部绘制工具**常显**（当前激活项高亮），不再做旧下拉的"隐藏当前工具"去重。
        // 第三项为 tooltip 所用的简短名称。
        let items: &[(ToolPanelItem, icon::Icon, &'static str, bool)] = &[
            (
                ToolPanelItem::Mouse,
                icon::MousePointer,
                t.tool_mouse,
                cur == Tool::ShapeSelect,
            ),
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

        // 工具设置下拉（画刷 / 形状 / 颜料桶分音符填充）：三者互斥，至多一个打开。
        // 随条目**挂在触发它的那个图标按钮**上（见下方循环），面板由此水平居中于按钮。
        let mut settings_menu = self.draw_tool_settings_menu();

        // 首项为**专用拖拽柄**（独立于按钮区，保证起拖信号不被按钮吞掉），
        // 其后为分隔竖线，再是全部绘制工具图标（常显 + 激活高亮）。
        let mut row_items: Vec<Element<'_>> = Vec::with_capacity(items.len() + 4);
        row_items.push(drag_handle());
        row_items.push(grip_divider());
        for (item, ic, desc, selected) in items {
            // 已启用的条目：再次点击 = 弹出该工具的设置（不再依赖 Ctrl）。
            let on_press = tool_panel_item_press(*item, *selected);
            let btn = tool_icon_button(*ic, *selected, on_press, &self.window.theme);

            // 打开着设置面板的条目：`CurveToolGroup` 的锚点 = 它的内容元素，
            // 故把**按钮**（而非整条胶囊）作为内容传入——面板才会居中于该按钮。
            let anchored: Element<'_> = match settings_menu.take_if(|(owner, ..)| *owner == *item) {
                Some((_, menu, close)) => {
                    // 面板配色 / 宽度与旧主工具栏下拉一致；点击面板内空白即关闭。
                    let panel: Element<'_> = mouse_area(menu).on_press(close).into();
                    CurveToolGroup::new(btn, Some(panel), MENU_WIDTH).into()
                }
                None => btn,
            };
            row_items.push(with_tooltip(anchored, desc));
        }
        // 不变式：设置面板的归属条目必在上表内（否则面板无处挂载、被静默丢弃）。
        debug_assert!(
            settings_menu.is_none(),
            "设置面板的归属条目不在条目列表内：面板将无处挂载"
        );

        // 右端「设置按钮」（齿轮）：以分隔线与工具图标区隔开，**常显**且不参与工具选择
        // 语义——它是各绘制工具设置的**聚合入口**，点开的是**独立 OS 窗口**
        // （`DialogType::DrawSettings`），因此这里只放一个纯按钮，不带锚定悬浮层、
        // 也不带开合高亮态（真实开关状态在独立窗口自己的标题栏上，主窗再维护一份会漂移）。
        row_items.push(grip_divider());
        row_items.push(with_tooltip(
            tool_icon_button(
                icon::Gear,
                false,
                Event::open_draw_settings_dialog(),
                &self.window.theme,
            ),
            t.tool_panel_settings,
        ));

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
        let pill_el: Element<'_> = mouse_area(pill)
            .on_press(Event::tool_panel_drag_started())
            .on_release(Event::tool_panel_drag_ended())
            .into();

        // 定位：水平居中（dx = 相对中心的偏移），底部对齐 + dy 内缩。
        // 用左右不等的 padding 制造中心平移：居中时平移量 = (left - right) / 2，
        // 故取 left / right = 2 × |dx| 即得所需位移 dx。
        let (dx, dy) = self.toolbar.tool_panel_offset;
        let centered = container(pill_el)
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

    /// 悬浮层小面板（工具设置 / 音符画设置）的统一底色：工具栏底色压暗 10%。
    ///
    /// 抽为单独方法，避免"画刷 / 形状 / 颜料桶面板"与"音符画设置面板"各自算一遍
    /// 而漂移出两种深浅（同族面板配色不一致会显得像两个系统）。
    fn draw_tool_panel_background(&self) -> Color {
        let toolbar_bg = self
            .window
            .theme
            .extended_palette()
            .background
            .weakest
            .color;
        Color::from_rgba(
            toolbar_bg.r * 0.9,
            toolbar_bg.g * 0.9,
            toolbar_bg.b * 0.9,
            toolbar_bg.a,
        )
    }

    /// 构建悬浮条的「工具设置」下拉（画刷 / 形状 / 颜料桶）。
    ///
    /// 返回 `(归属条目, 菜单元素, 点击菜单外空白时的关闭消息)`；仅当对应下拉处于打开态时
    /// 返回 `Some`（三者互斥）。**归属条目决定面板挂在哪个图标按钮上**：调用方用它
    /// 构造 `CurveToolGroup::new(该按钮, 菜单)`，面板即水平居中于该按钮（见模块头锚点说明）。
    /// 面板配色贴近工具栏（工具栏底色压暗 10%，见 `draw_tool_panel_background`），
    /// 与旧主工具栏入口按钮下拉保持一致观感。
    ///
    /// 颜料桶的「分音符填充」面板由 `state.fill_division_dialog.is_open` 驱动
    /// （画布 Ctrl+单击与悬浮条「再次点击颜料桶」两条路径共用同一状态），
    /// 渲染为与画刷 / 形状同风格的小面板——不再是全屏居中弹窗。
    fn draw_tool_settings_menu(&self) -> Option<(ToolPanelItem, Element<'_>, Message)> {
        let panel_background = self.draw_tool_panel_background();

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
            Some((ToolPanelItem::Brush, menu, Event::close_brush_dropdown()))
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
            Some((ToolPanelItem::Shape, menu, Event::close_shape_dropdown()))
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
            Some((
                ToolPanelItem::FillBucket,
                menu,
                Message::FillDivision(FillDivisionAction::CloseDialog),
            ))
        } else {
            None
        }
    }
}

/// 该条目是否有可弹出的「工具设置」。
///
/// 画刷 → 画刷设置下拉；形状 → 形状选择下拉；颜料桶 → 分音符填充面板。
/// 曲线 / 文字无独立设置，再次点击退化为普通选择。
fn has_settings(item: ToolPanelItem) -> bool {
    matches!(
        item,
        ToolPanelItem::Brush | ToolPanelItem::Shape | ToolPanelItem::FillBucket
    )
}

/// 条目点击 → 工具栏消息（**悬浮条点击契约的唯一出口**）。
///
/// - 条目**已启用**（工具已激活 / 填充已开启）且该条目有设置 → 弹出其设置面板；
/// - 其余情况 → 普通选择（未启用的条目首次点击 = 启用；无设置的条目恒为选择）。
///
/// 此前弹出的唯一入口是「Ctrl + 单击」，用户反馈该手势不可发现（"需要按 Ctrl"），
/// 且 Ctrl 状态在焦点切换等场景下并不可靠（见 `Editor::ctrl_pressed` 的双通道兜底注释）。
/// 现改为「启用后再次点击」，与 `selected` 高亮状态构成自解释的两段式交互。
fn tool_panel_item_press(item: ToolPanelItem, enabled: bool) -> Message {
    if enabled && has_settings(item) {
        Event::tool_panel_item_settings_requested(item)
    } else {
        Event::tool_panel_item_selected(item)
    }
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

/// 构建面板中的图标独占按钮**本体**（图标 + 尺寸 + 选中态样式，不含 tooltip）。
///
/// 与 tooltip 分开构造：设置面板必须挂在**按钮**这一层（`CurveToolGroup` 的锚点 =
/// 其内容元素），先挂面板、后包 tooltip，锚点语义才与"触发它的按钮"严格一致。
///
/// `on_press` 由 [`tool_panel_item_press`] 给出：普通选择或「启用后再次点击 = 弹出设置」。
fn tool_icon_button(
    ic: icon::Icon,
    selected: bool,
    on_press: Message,
    theme: &Theme,
) -> Element<'static> {
    let icon_el = icon::view_with_size_and_theme(ic, ICON_SIZE, ICON_SIZE, Some(theme));
    button(icon_el)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(on_press)
        .style(move |theme: &Theme, status| button_style(theme, status, selected))
        .into()
}

/// 给面板条目挂上悬浮 tooltip（文案 = 工具名；避免把文字塞进按钮撑宽）。
///
/// 生命周期对内容泛化：内容可能是"按钮"本体，也可能是包了设置面板悬浮层的
/// `CurveToolGroup`（其菜单元素借用了 `&self`）。
fn with_tooltip<'a>(content: Element<'a>, desc: &'static str) -> Element<'a> {
    tooltip::Tooltip::new(content, text(desc), tooltip::Position::Top)
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
    use iced_core::{Font, Pixels, Rectangle, Size, Vector, layout, widget};
    use lumino_core::storage::config::UiConfig;

    /// 构建 **headless** iced 渲染器（不需要窗口）——悬浮层几何断言必须有真实布局参与。
    ///
    /// 无可用 GPU 适配器时返回 `None`，调用方**跳过**而不是失败：CI 软渲染环境不保证
    /// 有适配器（与 `lumino-gfx` 的 `LUMINO_GFX_TEST_FALLBACK` 同一考虑）。
    fn headless_renderer() -> Option<crate::Renderer> {
        use iced_wgpu::graphics::Shell;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        let adapter = rt
            .block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
        let (device, queue) = rt
            .block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("lumino_ui_overlay_test_device"),
                required_features: adapter.features() & wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))
            .ok()?;
        let engine = iced_wgpu::Engine::new(
            &adapter,
            device,
            queue,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            None,
            Shell::headless(),
        );

        Some(crate::Renderer::new(
            engine,
            Font::DEFAULT,
            Pixels::from(16),
        ))
    }

    /// 沿「单子节点」链下行，定位恰好含 `count` 个子节点的布局节点
    /// （= 胶囊内的条目行 `row(...)`；沿途 container / mouse_area / tooltip /
    /// CurveToolGroup 都是单子节点转发，不改变行结构）。
    ///
    /// 用 `Layout` 而非裸 `Node`：后者子节点坐标**相对父节点**，前者是绝对坐标。
    fn find_row(layout: layout::Layout<'_>, count: usize) -> Option<layout::Layout<'_>> {
        if layout.children().len() == count {
            return Some(layout);
        }
        let mut children = layout.children();
        match (children.next(), children.next()) {
            (Some(only), None) => find_row(only, count),
            _ => None,
        }
    }

    /// 在悬浮层节点树中定位**最外层**宽度 ≈ `width` 的节点 = 工具设置面板本体。
    ///
    /// 悬浮层根节点是承载层（container / tooltip 的 `overlay::Group`），其 bounds 被
    /// 撑满整个视口（`Group::layout` 用 `bounds` 作为自身尺寸），真正的位置 / 宽度在
    /// 下钻后的面板节点上。
    fn find_panel(layout: layout::Layout<'_>, width: f32) -> Option<layout::Layout<'_>> {
        if (layout.bounds().width - width).abs() <= 1.0 {
            return Some(layout);
        }
        layout.children().find_map(|child| find_panel(child, width))
    }

    /// 条目行内的固定槽位数（也是 `find_row` 的定位依据）：
    /// `0` = 拖拽柄、`1` = 分隔线、`2..=7` = 6 个绘制工具、`8` = 分隔线、
    /// `9` = 右端「设置按钮」（齿轮）。
    const ROW_SLOTS: usize = 2 + 6 + 2;
    /// 右端「设置按钮」（齿轮）在条目行内的槽位号。
    const GEAR_SLOT: usize = ROW_SLOTS - 1;

    /// 回归测试：工具自带设置面板必须**水平居中于触发它的那个按钮**。
    ///
    /// 用户反馈 BUG：「弹出的悬浮面板没有居中对齐按钮，而是直接出现在了悬浮工具栏
    /// 左端对齐处」。根因：`CurveToolGroup` 的锚点 = 它的内容元素，而此前传入的内容
    /// 是**整条胶囊**（宽 265），面板（宽 248）于是以"胶囊中心"为准居中——
    /// `(265 - 248) / 2 = 8.5`，看起来就是贴在悬浮条左端；与真正被点的按钮中心
    /// （形状按钮在胶囊内偏移 205）相差 70 余像素。
    ///
    /// 注意：**齿轮不在本测试范围内**——它点开的是独立 OS 窗口（`DialogType::DrawSettings`），
    /// 主窗口内不存在属于它的悬浮层，没有"居中于按钮"这回事。
    #[test]
    fn test_settings_panel_is_centered_on_its_trigger_button() {
        let Some(renderer) = headless_renderer() else {
            eprintln!("跳过：无可用 GPU 适配器（悬浮层几何断言需要真实布局）");
            return;
        };

        // 三个有窗口内设置的入口各验一次：面板必须居中于**各自**的按钮（而非某个固定位置）。
        // 槽位号见上方 `ROW_SLOTS` 注释；
        // items 顺序：Mouse(0) Curve(1) FillBucket(2) Brush(3) Shape(4) Text(5)
        check_panel_center(&renderer, 2 + 3, "画刷", |root| {
            root.toolbar.brush_dropdown_open = true;
        });
        check_panel_center(&renderer, 2 + 4, "形状", |root| {
            root.toolbar.shape_dropdown_open = true;
        });
        check_panel_center(&renderer, 2 + 2, "颜料桶", |root| {
            root.state.fill_division_dialog.is_open = true;
        });
    }

    /// 齿轮必须落在胶囊**最右端**（工具图标区之后），且其按钮宽度与其它条目同规格。
    ///
    /// 需求明确要求"右端设置按钮"：这条几何断言把它钉死——若哪天有人把齿轮挪进工具
    /// 图标之间（比如插到文字工具后面），中心 x 不再最大，本测试立刻失败。
    #[test]
    fn test_gear_entry_sits_at_right_end() {
        let Some(renderer) = headless_renderer() else {
            eprintln!("跳过：无可用 GPU 适配器（几何断言需要真实布局）");
            return;
        };

        let mut root = Root::new(&UiConfig::default());
        root.toolbar.tool_panel_open = true;
        let mut element = root.view_draw_toolbar().expect("悬浮条已打开应渲染");
        let mut tree = widget::Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(1400.0, 900.0));
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let row = find_row(layout::Layout::new(&node), ROW_SLOTS)
            .expect("应能定位胶囊内的条目行（拖拽柄 + 分隔线 + 6 工具 + 分隔线 + 齿轮）");

        let gear = row
            .children()
            .nth(GEAR_SLOT)
            .expect("齿轮应有布局节点")
            .bounds();
        // 参与比较的只有 6 个工具按钮（槽位 2..=7）——拖拽柄/分隔线不算"工具图标区"。
        let tool_max_right = (2..2 + 6)
            .filter_map(|i| row.children().nth(i))
            .map(|c| c.bounds().x + c.bounds().width)
            .fold(f32::MIN, f32::max);

        assert!(
            gear.x >= tool_max_right,
            "齿轮应在全部工具图标的右侧：齿轮左缘 {:.1} vs 工具区右缘 {:.1}",
            gear.x,
            tool_max_right
        );
        assert!(
            (gear.width - BUTTON_SIZE).abs() < 1.0 && (gear.height - BUTTON_SIZE).abs() < 1.0,
            "齿轮按钮应为标准图标按钮尺寸 {BUTTON_SIZE}，实际 {:.1}x{:.1}",
            gear.width,
            gear.height
        );
    }

    /// 校验「设置面板水平居中于第 `slot` 个条目（0 = 拖拽柄）的按钮」。
    fn check_panel_center(
        renderer: &crate::Renderer,
        slot: usize,
        label: &str,
        configure: impl FnOnce(&mut Root),
    ) {
        let mut root = Root::new(&UiConfig::default());
        root.toolbar.tool_panel_open = true;
        configure(&mut root);

        let mut element = root.view_draw_toolbar().expect("悬浮条已打开应渲染");
        let mut tree = widget::Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(1400.0, 900.0));
        let node = element.as_widget_mut().layout(
            &mut tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );

        // 条目行：0 = 拖拽柄，1 = 分隔线，其后 6 个工具按钮（顺序同上方 `items`），
        // 再 1 个分隔线，末位 = 右端设置按钮（齿轮）
        let row = find_row(layout::Layout::new(&node), ROW_SLOTS)
            .expect("应能定位胶囊内的条目行（拖拽柄 + 分隔线 + 6 工具 + 分隔线 + 齿轮）");
        let button = row.children().nth(slot).expect("条目应有布局节点").bounds();

        let mut overlay = element
            .as_widget_mut()
            .overlay(
                &mut tree,
                layout::Layout::new(&node),
                renderer,
                &viewport,
                Vector::ZERO,
            )
            .expect("设置面板打开时应存在悬浮层");
        let overlay_node = overlay.as_overlay_mut().layout(renderer, viewport.size());
        // 悬浮层根节点 = 承载层（占满视口），下钻到面板本体（宽度 = MENU_WIDTH）。
        let panel = find_panel(layout::Layout::new(&overlay_node), MENU_WIDTH)
            .expect("悬浮层内应能找到设置面板本体")
            .bounds();

        assert!(
            (panel.center_x() - button.center_x()).abs() <= 1.0,
            "{label}设置面板必须水平居中于触发按钮：面板中心 {:.1} vs 按钮中心 {:.1}\
             （面板 x {:.1}..{:.1}，按钮 x {:.1}..{:.1}）",
            panel.center_x(),
            button.center_x(),
            panel.x,
            panel.x + panel.width,
            button.x,
            button.x + button.width,
        );
    }

    /// 面板宽度护栏：分音符填充面板（含「关闭填充」按钮）在中文 / 英文下内容宽度都不得
    /// 超过 `MENU_WIDTH`——面板由调用方固定为 `MENU_WIDTH`，内容超宽会溢出面板背景。
    #[test]
    fn test_fill_division_dropdown_fits_menu_width() {
        use lumino_extras::i18n::Language;

        let Some(renderer) = headless_renderer() else {
            eprintln!("跳过：无可用 GPU 适配器（文本度量需要真实渲染器）");
            return;
        };

        for lang in [Language::ZhCn, Language::EnUs] {
            let mut element = crate::toolbar::fill_division_dropdown::render_fill_division_dropdown(
                "16",
                lang,
                Color::from_rgba(0.1, 0.1, 0.1, 1.0),
                &crate::Theme::Dark,
            );
            let mut tree = widget::Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::INFINITE),
            );
            let width = node.bounds().width;
            assert!(
                width <= MENU_WIDTH,
                "{lang:?} 下面板内容宽度 {width:.1} 超出 MENU_WIDTH {MENU_WIDTH}"
            );
        }
    }

    /// 该消息是否为「弹出设置」请求（`Message` 未实现 `PartialEq`，按变体精确匹配）
    fn is_settings_request(msg: &Message, item: ToolPanelItem) -> bool {
        matches!(
            msg,
            Message::Toolbar(Event::ToolPanelItemSettingsRequested(i)) if *i == item
        )
    }

    /// 该消息是否为普通选择
    fn is_plain_select(msg: &Message, item: ToolPanelItem) -> bool {
        matches!(msg, Message::Toolbar(Event::ToolPanelItemSelected(i)) if *i == item)
    }

    /// 点击契约：**已启用**条目再次点击 → 设置请求；其余 → 普通选择。
    ///
    /// 这是"不再需要 Ctrl+单击"的视图层唯一出口，逐条钉死。
    #[test]
    fn test_tool_panel_item_press_contract() {
        const ALL: [ToolPanelItem; 7] = [
            ToolPanelItem::Mouse,
            ToolPanelItem::StrokeSettings,
            ToolPanelItem::Curve,
            ToolPanelItem::FillBucket,
            ToolPanelItem::Brush,
            ToolPanelItem::Shape,
            ToolPanelItem::Text,
        ];

        for item in ALL {
            assert!(
                is_plain_select(&tool_panel_item_press(item, false), item),
                "未启用条目 {item:?} 首次点击应为普通选择（启用）"
            );
        }

        // 已启用 + 有设置：再次点击 = 弹出该条目的设置面板
        for item in [
            ToolPanelItem::Brush,
            ToolPanelItem::Shape,
            ToolPanelItem::FillBucket,
        ] {
            assert!(
                is_settings_request(&tool_panel_item_press(item, true), item),
                "已启用条目 {item:?} 再次点击应弹出设置"
            );
        }

        // 已启用但无独立设置：仍是普通选择（不产生空的设置请求）
        for item in [
            ToolPanelItem::Mouse,
            ToolPanelItem::StrokeSettings,
            ToolPanelItem::Curve,
            ToolPanelItem::Text,
        ] {
            assert!(
                is_plain_select(&tool_panel_item_press(item, true), item),
                "无设置条目 {item:?} 再次点击应退化为普通选择"
            );
        }
    }

    /// 右端「设置按钮」（齿轮）的点击契约：**只请求打开独立对话框**。
    ///
    /// 齿轮是独立于 6 个工具条目的**入口按钮**，有两条不能破的语义：
    /// 1. 不得退化为工具选择（否则点设置会顺手把当前绘制工具切掉——用户只是想调参数）；
    /// 2. 不得在主窗口内落任何浮层（面板已改为独立 OS 窗口，见 `DialogType::DrawSettings`），
    ///    故消息里**只有** `OpenDrawSettingsDialog`，没有配套的"关闭/切换"事件。
    #[test]
    fn test_gear_settings_button_contract() {
        assert!(
            matches!(
                Event::open_draw_settings_dialog(),
                Message::Toolbar(Event::OpenDrawSettingsDialog)
            ),
            "齿轮按钮按下应发 OpenDrawSettingsDialog"
        );
    }

    /// 设置面板的**归属条目**：决定面板挂在哪个图标按钮上（锚点正确性的上游）。
    #[test]
    fn test_tool_settings_menu_owner() {
        let mut root = Root::new(&UiConfig::default());
        root.toolbar.tool_panel_open = true;
        assert!(
            root.draw_tool_settings_menu().is_none(),
            "无下拉打开时不应有设置面板"
        );

        root.toolbar.brush_dropdown_open = true;
        assert_eq!(
            root.draw_tool_settings_menu().map(|(owner, ..)| owner),
            Some(ToolPanelItem::Brush)
        );
        root.toolbar.brush_dropdown_open = false;

        root.toolbar.shape_dropdown_open = true;
        assert_eq!(
            root.draw_tool_settings_menu().map(|(owner, ..)| owner),
            Some(ToolPanelItem::Shape)
        );
        root.toolbar.shape_dropdown_open = false;

        root.state.fill_division_dialog.is_open = true;
        assert_eq!(
            root.draw_tool_settings_menu().map(|(owner, ..)| owner),
            Some(ToolPanelItem::FillBucket)
        );
    }

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
        assert!(!has_settings(ToolPanelItem::Mouse));
    }
}
