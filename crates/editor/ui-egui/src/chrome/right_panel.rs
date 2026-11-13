//! 右侧栏面板内容（图片转 MIDI / 素材库）+ 动作通道。
//!
//! 面板几何与生产一致：总宽 `panel_width` = 左侧 6px 拖拽手柄 + 内容区
//! `panel_width - 6`，右侧是固定 48px 图标列。内容区整块进入占用列表
//! （拖拽手柄除外），点击不会再落到下层 旧版 的同名控件；滚动由各面板自己的
//! `ScrollArea` 处理。
//!
//! 动作一帧只保留第一个（先到先得），避免面板重排时新控件被同一帧的点击连带
//! 触发；素材拖出只在**按下瞬间**回传 `MaterialDragStarted`，跟随预览与松手
//! 放置继续由 旧版 编辑器状态机驱动。

use egui::{Id, Rect};

use super::model::RightSidebarPanelModel;
use crate::icons::IconCache;

mod hits;
mod i2m;
mod materials;
mod waterfall;
mod widgets;

pub use hits::{button_rect, clear_panel_rect, field_rect, panel_rect, rect};
pub(crate) use hits::{clear_rects, record_rect, set_panel_rect};

const ACTION_ID: &str = "lumino.right-sidebar.action";

/// 右侧栏面板产生的动作（不依赖 旧版）。
#[derive(Debug, Clone, PartialEq)]
pub enum RightSidebarAction {
    /// 点击图标列按钮（切换/展开对应面板）。
    RailClicked(super::model::RightSidebarPanelKind),
    /// 按下 6px 缩放手柄（开始拖拽面板宽度）。
    ResizeStarted,
    /// 松开缩放手柄（结束拖拽）。
    ResizeEnded,
    /// 点击「选择图片文件」。
    SelectImageFile,
    /// 配置文本变化（原样回传，生产负责合法性校验）。
    I2mConfigTextChanged {
        /// 字段。
        field: super::model::I2mField,
        /// 新文本。
        text: String,
    },
    /// 调色板算法切换。
    I2mPaletteChanged(usize),
    /// 点击「转换为 MIDI」。
    ConvertClicked,
    /// 开合「添加素材」下拉菜单。
    MaterialAdd,
    /// 选择添加下拉菜单项。
    MaterialMenu(super::model::MaterialMenuKind),
    /// 素材项按下（开始拖出到卷帘）。
    MaterialDragStarted(usize),
    /// 素材项右键（面板局部坐标，用于定位菜单）。
    MaterialContextMenuOpened {
        /// 素材列表索引。
        index: usize,
        /// 面板局部坐标 x。
        x: f32,
        /// 面板局部坐标 y。
        y: f32,
    },
    /// 关闭素材右键菜单（点击空白 / 覆盖背景）。
    MaterialContextMenuClosed,
    /// 点击素材右键菜单项。
    MaterialContextMenuItemClicked {
        /// 素材列表索引。
        index: usize,
        /// 菜单项。
        item: super::model::MaterialContextMenuItem,
    },
    /// 素材重命名输入变化。
    MaterialRenameInput(String),
    /// 确认素材重命名（回车）。
    MaterialRenameConfirmed,
}

/// 记录本帧要回传给生产的动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: RightSidebarAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<RightSidebarAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<RightSidebarAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<RightSidebarAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 绘制 6px 缩放手柄并接管拖拽（按下/松开 → 生产侧 start/end_resize）。
///
/// 宽度跟随仍由宿主处理：旧版 每条 `CursorMoved` 都会调用
/// `RightSidebar::update_resize_position`，这里只需正确发出起止信号。
pub fn show_handle(ui: &egui::Ui, rect: Rect, resizing: bool) {
    ui.painter().rect_filled(
        rect,
        0,
        if resizing {
            crate::theme::accent_strong()
        } else {
            crate::theme::panel_bg()
        },
    );
    let response = ui.interact(
        rect,
        Id::unique("lumino.right-sidebar.handle"),
        egui::Sense::click_and_drag(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

    // 起止信号必须**成对**发出：不能依赖生产状态 `resizing` 的时序——旧版 在
    // 鼠标松开时会先走 `handle_release_all` 直接 `end_resize()`，等到下一帧
    // `resizing` 已经是 false，就再也发不出 Ended。这里用 egui 自己的边沿状态。
    let dragging_id = Id::unique("lumino.right-sidebar.handle.dragging");
    let started = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(dragging_id))
        .unwrap_or(false);
    let down = response.is_pointer_button_down_on();
    if down && !started {
        ui.ctx().data_mut(|d| d.insert_temp(dragging_id, true));
        push_action(ui.ctx(), RightSidebarAction::ResizeStarted);
    } else if !down && started {
        ui.ctx().data_mut(|d| d.insert_temp(dragging_id, false));
        push_action(ui.ctx(), RightSidebarAction::ResizeEnded);
    }

    // 手柄由 egui 独占：旧版 的 mouse_area 不再收到按下/松开。
    super::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 绘制面板内容（`rect` = 手柄右侧的内容区）。
pub fn show(ui: &mut egui::Ui, rect: Rect, panel: &RightSidebarPanelModel, icons: &mut IconCache) {
    ui.painter().rect_filled(rect, 0, crate::theme::panel_bg());
    clear_rects(ui.ctx());
    match panel {
        RightSidebarPanelModel::ImageToMidi(model) => i2m::show(ui, rect, model),
        RightSidebarPanelModel::Materials(model) => materials::show(ui, rect, model, icons),
        RightSidebarPanelModel::PianoWaterfall(model) => waterfall::show(ui, rect, model),
    }
    // 内容区整块占用：点击 / 滚轮只给 egui。拖拽手柄（内容区左侧 6px）不在
    // 本矩形内，旧版 仍能收到缩放拖拽。
    super::menu::extend_blocked(ui.ctx(), [rect]);
}
