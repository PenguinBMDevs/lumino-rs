//! 钢琴卷帘右键上下文菜单
//!
//! 提供钢琴卷帘区域内右键弹出的内嵌悬浮面板菜单。
//! 菜单以垂直图标栏形式显示在右键位置附近，
//! 模仿 ../yinhe 的选中后悬浮面板风格。

use iced_core::{Alignment, Color, Length, Padding, Point, Size};
use iced_widget::{Space, button, column, container, mouse_area, tooltip};
use lumino_core::tr;
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction, PianoRollContextMenuItem};

use crate::{Element, Message, Theme};

/// 图标按钮尺寸（宽高相同）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 面板宽度
const PANEL_WIDTH: f32 = BUTTON_SIZE + PANEL_PADDING * 2.0;
/// 面板与鼠标位置的水平偏移（避免遮挡光标）
const MENU_OFFSET_X: f32 = 16.0;
/// 面板与鼠标位置的垂直偏移
const MENU_OFFSET_Y: f32 = 16.0;

/// 深色菜单背景，保证在浅色主题下也能明显区分
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// Tooltip 深色背景
const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.08, 0.08, 0.10, 0.96);
/// 浅色悬停/按下颜色，用于深色按钮背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字，用于深色 Tooltip 背景
const TOOLTIP_TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 钢琴卷帘上下文菜单状态
#[derive(Debug, Clone, Default)]
pub struct PianoRollContextMenuState {
    /// 菜单是否打开
    pub open: bool,
    /// 菜单打开位置（canvas 局部坐标）
    pub position: Option<Point>,
    /// 菜单作用目标（决定渲染的条目集）
    pub target: ContextMenuTarget,
}

impl PianoRollContextMenuState {
    /// 打开菜单（指定作用目标）
    pub fn open(&mut self, position: Point, target: ContextMenuTarget) {
        self.open = true;
        self.position = Some(position);
        self.target = target;
    }

    /// 关闭菜单
    pub fn close(&mut self) {
        self.open = false;
        self.position = None;
    }

    /// 切换菜单状态
    #[allow(dead_code)]
    pub fn toggle(&mut self, position: Point, target: ContextMenuTarget) {
        if self.open {
            self.close();
        } else {
            self.open(position, target);
        }
    }
}

/// 渲染上下文菜单覆盖层
///
/// `available` 为编辑器可用区域；菜单会左右夹取，并在下方放不下时向上翻转，
/// 避免条目被窗口底边裁掉（041C 待确认菜单已有 11 项）。
pub fn view(position: Point, target: ContextMenuTarget, available: Size) -> Element<'static> {
    let panel_height = menu_total_height(target);
    let panel_width = PANEL_WIDTH + PANEL_PADDING * 2.0;
    let x = (position.x + MENU_OFFSET_X).clamp(
        0.0,
        (available.width - panel_width - MENU_OFFSET_X).max(0.0),
    );
    let below = position.y + MENU_OFFSET_Y;
    let y = if below + panel_height > available.height {
        (position.y - panel_height - MENU_OFFSET_Y).max(0.0)
    } else {
        below
    };
    let y = y.min((available.height - panel_height).max(0.0));

    let menu_panel = menu_panel(target);

    container(menu_panel)
        .padding(Padding {
            top: y,
            right: 0.0,
            bottom: 0.0,
            left: x,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个编辑器区域，点击时关闭菜单。
/// 菜单面板应放在此背景之上，使菜单内部的点击由按钮处理。
pub fn background_close_overlay<'a>() -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::PianoRollContextMenu(
            PianoRollContextMenuAction::Close,
        ))
        .into()
}

/// 目标对应的菜单条目集
fn menu_items(target: ContextMenuTarget) -> &'static [PianoRollContextMenuItem] {
    match target {
        ContextMenuTarget::Notes => &[
            PianoRollContextMenuItem::Cut,
            PianoRollContextMenuItem::Copy,
            PianoRollContextMenuItem::Paste,
            PianoRollContextMenuItem::Delete,
            PianoRollContextMenuItem::SelectAll,
            PianoRollContextMenuItem::BatchEdit,
            PianoRollContextMenuItem::ShapeList,
            PianoRollContextMenuItem::PasteShapeText,
        ],
        ContextMenuTarget::DrawnShape => &[
            PianoRollContextMenuItem::LiveShape,
            PianoRollContextMenuItem::SplitLiveShape,
            PianoRollContextMenuItem::JoinShapes,
            PianoRollContextMenuItem::SplitJoinedShape,
            PianoRollContextMenuItem::MergeShapes,
            PianoRollContextMenuItem::EditShape,
            PianoRollContextMenuItem::PaintAreas,
            PianoRollContextMenuItem::MaskToggle,
            PianoRollContextMenuItem::SymmetryVertical,
            PianoRollContextMenuItem::SymmetryHorizontal,
            PianoRollContextMenuItem::Straighten,
            PianoRollContextMenuItem::StraightenOff,
            PianoRollContextMenuItem::SliceVertical,
            PianoRollContextMenuItem::BetweenCreate,
            PianoRollContextMenuItem::CopyAlong,
            PianoRollContextMenuItem::VelocityUp,
            PianoRollContextMenuItem::CopyShapeText,
            PianoRollContextMenuItem::PasteShapeText,
            PianoRollContextMenuItem::DeleteDrawnShape,
        ],
        // 待确认镜像没有 ShapeStore 模型；DRAW-041C 起走 Bake 前组合会话。
        // 019/021/022 需 2+ 个 pending；025/027/028 单对象；026 需恰好 2 个。
        ContextMenuTarget::PendingDrawnShape => &[
            PianoRollContextMenuItem::LiveShape,
            PianoRollContextMenuItem::JoinShapes,
            PianoRollContextMenuItem::MergeShapes,
            PianoRollContextMenuItem::EditShape,
            PianoRollContextMenuItem::MaskToggle,
            PianoRollContextMenuItem::SliceVertical,
            PianoRollContextMenuItem::BetweenCreate,
            PianoRollContextMenuItem::CopyAlong,
            PianoRollContextMenuItem::VelocityUp,
            PianoRollContextMenuItem::CopyShapeText,
            PianoRollContextMenuItem::DeleteDrawnShape,
        ],
    }
}

/// 菜单总高度（含内边距）
fn menu_total_height(target: ContextMenuTarget) -> f32 {
    let count = menu_items(target).len();
    count as f32 * BUTTON_SIZE
        + count.saturating_sub(1) as f32 * BUTTON_SPACING
        + PANEL_PADDING * 2.0
}

/// 构建菜单面板内容（条目集由作用目标决定）
fn menu_panel(target: ContextMenuTarget) -> Element<'static> {
    let items = menu_items(target);
    let buttons = items.iter().copied().map(menu_button).collect::<Vec<_>>();

    let total_height = menu_total_height(target);

    let panel = container(
        column(buttons)
            .spacing(BUTTON_SPACING)
            .align_x(Alignment::Center),
    )
    .padding(PANEL_PADDING)
    .width(Length::Fixed(PANEL_WIDTH))
    .height(Length::Fixed(total_height))
    .style(|_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(PANEL_BACKGROUND)),
        border: iced_core::Border::default().rounded(8),
        ..Default::default()
    });

    // 用 mouse_area 包裹面板，吞掉菜单背景上的点击事件，
    // 避免触发下层的关闭覆盖层。按钮作为子元素仍会优先响应点击。
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建单个菜单按钮
fn menu_button(item: PianoRollContextMenuItem) -> Element<'static> {
    // 面板固定为深色，图标始终按暗色主题反色渲染，保证亮/暗主题下均为浅色可见
    let icon = lumino_ui_core::resources::icon::view_with_size_and_theme(
        item_icon(item),
        ICON_SIZE,
        ICON_SIZE,
        Some(&Theme::Dark),
    );

    let tooltip_text = item_label(item);

    let btn = button(icon)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(Message::PianoRollContextMenu(
            PianoRollContextMenuAction::ItemClicked(item),
        ))
        .style(|_theme: &Theme, status| button_style(status));

    tooltip::Tooltip::new(btn, tooltip_text, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 菜单项对应的图标
const fn item_icon(item: PianoRollContextMenuItem) -> lumino_ui_core::resources::icon::Icon {
    use lumino_ui_core::resources::icon::Icon;
    match item {
        PianoRollContextMenuItem::Cut => Icon::ContextMenuCut,
        PianoRollContextMenuItem::Copy => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::Paste => Icon::ContextMenuPaste,
        PianoRollContextMenuItem::Delete => Icon::ContextMenuDelete,
        PianoRollContextMenuItem::SelectAll => Icon::ContextMenuSelectAll,
        PianoRollContextMenuItem::BatchEdit => Icon::Gear,
        PianoRollContextMenuItem::DeleteDrawnShape => Icon::ContextMenuDelete,
        PianoRollContextMenuItem::LiveShape => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::SplitLiveShape => Icon::ContextMenuPaste,
        PianoRollContextMenuItem::JoinShapes => Icon::Gear,
        PianoRollContextMenuItem::SplitJoinedShape => Icon::ContextMenuCut,
        PianoRollContextMenuItem::MergeShapes => Icon::ContextMenuSelectAll,
        PianoRollContextMenuItem::EditShape => Icon::Gear,
        PianoRollContextMenuItem::PaintAreas => Icon::Gear,
        PianoRollContextMenuItem::MaskToggle => Icon::Gear,
        PianoRollContextMenuItem::ShapeList => Icon::Gear,
        PianoRollContextMenuItem::SymmetryVertical => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::SymmetryHorizontal => Icon::ContextMenuPaste,
        PianoRollContextMenuItem::Straighten => Icon::ContextMenuCut,
        PianoRollContextMenuItem::StraightenOff => Icon::ContextMenuDelete,
        PianoRollContextMenuItem::SliceVertical => Icon::ContextMenuCut,
        PianoRollContextMenuItem::BetweenCreate => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::CopyAlong => Icon::ContextMenuPaste,
        PianoRollContextMenuItem::VelocityUp => Icon::ContextMenuSelectAll,
        PianoRollContextMenuItem::CopyShapeText => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::PasteShapeText => Icon::ContextMenuPaste,
    }
}

/// 菜单项显示文本
fn item_label(item: PianoRollContextMenuItem) -> &'static str {
    match item {
        PianoRollContextMenuItem::Cut => tr!("剪切"),
        PianoRollContextMenuItem::Copy => tr!("复制"),
        PianoRollContextMenuItem::Paste => tr!("粘贴"),
        PianoRollContextMenuItem::Delete => tr!("删除"),
        PianoRollContextMenuItem::SelectAll => tr!("全选"),
        PianoRollContextMenuItem::BatchEdit => tr!("批量编辑"),
        PianoRollContextMenuItem::DeleteDrawnShape => tr!("删除"),
        PianoRollContextMenuItem::LiveShape => tr!("转为活形状"),
        PianoRollContextMenuItem::SplitLiveShape => tr!("还原活形状"),
        PianoRollContextMenuItem::JoinShapes => tr!("连接图形"),
        PianoRollContextMenuItem::SplitJoinedShape => tr!("还原连接"),
        PianoRollContextMenuItem::MergeShapes => tr!("门控合并"),
        PianoRollContextMenuItem::EditShape => tr!("编辑图形"),
        PianoRollContextMenuItem::PaintAreas => tr!("区域着色"),
        PianoRollContextMenuItem::MaskToggle => tr!("切换遮罩"),
        PianoRollContextMenuItem::ShapeList => tr!("形状列表"),
        PianoRollContextMenuItem::SymmetryVertical => tr!("竖轴对称"),
        PianoRollContextMenuItem::SymmetryHorizontal => tr!("横轴对称"),
        PianoRollContextMenuItem::Straighten => tr!("扶正 50%"),
        PianoRollContextMenuItem::StraightenOff => tr!("取消扶正"),
        PianoRollContextMenuItem::SliceVertical => tr!("竖切"),
        PianoRollContextMenuItem::BetweenCreate => tr!("创建补间"),
        PianoRollContextMenuItem::CopyAlong => tr!("沿路径复制"),
        PianoRollContextMenuItem::VelocityUp => tr!("力度渐强"),
        PianoRollContextMenuItem::CopyShapeText => tr!("复制为分享文本"),
        PianoRollContextMenuItem::PasteShapeText => tr!("从分享文本粘贴"),
    }
}

/// 按钮样式
fn button_style(status: button::Status) -> button::Style {
    use button::Status;

    let background = match status {
        Status::Hovered => HOVER_BACKGROUND,
        Status::Pressed => PRESSED_BACKGROUND,
        _ => Color::TRANSPARENT,
    };

    button::Style {
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    }
    .with_background(background)
}

/// Tooltip 样式：深色背景 + 浅色文字
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced_core::Background::Color(TOOLTIP_BACKGROUND)),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(TOOLTIP_TEXT_COLOR),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
