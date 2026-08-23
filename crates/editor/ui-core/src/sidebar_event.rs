//! Sidebar 事件子模块
//!
//! 包括侧边栏事件枚举及其依赖的分组和路由类型。
//!
//! 子模块组织（保持单文件 < 400 行）：
//! - `group`: 分组 ID（`GroupId`）及灯条颜色 / 提示文本
//! - `route`: 路由（`Route`）与卷帘面板底部按钮（`RollBarButton`）
//! - `constructors`: `Event` 的 `Message` 构造器
//! - `tests`: 构造器与提示文本单元测试

mod constructors;
mod group;
mod route;

pub use group::GroupId;
pub use route::{RollBarButton, Route};

#[cfg(test)]
mod tests;

use iced_core::{Color, Point};
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};

// ─── 事件（从 sidebar/event.rs 迁入） ───

/// 侧边栏事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 路由更新
    RouteUpdated(Route),
    /// 面板切换
    PanelToggled(Route),
    /// 音轨选择
    TrackSelected(usize),
    /// 音轨静音切换
    TrackMuteToggled(usize),
    /// 音轨独奏切换
    TrackSoloToggled(usize),
    /// 音轨增益变化（线性，1.0 = 0 dB；负数按 0 处理）
    TrackGainChanged(usize, f32),
    /// 音轨声像变化（-1..1，0 = 居中）
    TrackPanChanged(usize, f32),
    /// 混音台浮动面板开关切换
    MixerPanelToggled,
    /// 混音台浮动面板最大化/最小化（展开/收起主体）切换
    MixerPanelMaximizeToggled,
    /// 混音台浮动面板拖拽（鼠标移动时的相对坐标，单位为逻辑像素）
    MixerPanelDragged(f32, f32),
    /// 混音台浮动面板拖拽开始（标题栏按下）
    MixerPanelDragStarted,
    /// 混音台浮动面板拖拽结束（松开）
    MixerPanelDragEnded,
    /// 多轨同时选择
    TracksSelected(Vec<usize>),
    /// 添加音轨
    AddTrack,
    /// 在指定音轨上方添加
    TrackAddAbove(usize),
    /// 在指定音轨下方添加
    TrackAddBelow(usize),
    /// 上移指定音轨
    TrackMoveUp(usize),
    /// 下移指定音轨
    TrackMoveDown(usize),
    /// 开始拖拽调整面板宽度
    ResizeDragStarted(Point),
    /// 拖拽中调整面板宽度
    ResizeDragged(Point),
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
    /// 自动化面板切换
    AutomationPanelToggled,
    /// 钢琴卷帘面板切换
    PianoRollToggled,
    /// 分组切换
    GroupToggled(GroupId),
    /// 卷帘面板底部按钮切换（横向/纵向三条杠，两者互斥）
    RollBarToggled(RollBarButton),
    /// 打开音轨选项卡右键菜单
    TrackContextMenuOpened(usize),
    /// 关闭音轨选项卡右键菜单
    TrackContextMenuClosed,
    /// 点击音轨选项卡右键菜单项
    TrackContextMenuItemClicked(usize, TrackContextMenuItem),
    /// 打开音轨列表面板空白区域右键菜单
    ///
    /// 注意：iced 0.14 的 `mouse_area::on_right_press` 仅传递 Message，
    /// 无法获取点击坐标。菜单固定显示在面板右上角（由 `panel_context_menu`
    /// 模块的 `positioned_menu` 决定）。
    PanelContextMenuOpened,
    /// 关闭音轨列表面板空白区域右键菜单
    PanelContextMenuClosed,
    /// 点击音轨列表面板空白区域右键菜单项
    PanelContextMenuItemClicked(PanelContextMenuItem),
    /// 开始重命名音轨
    TrackRenameStarted(usize),
    /// 重命名输入变化
    TrackRenameChanged(usize, String),
    /// 确认重命名
    TrackRenameConfirmed(usize),
    /// 取消重命名
    TrackRenameCancelled(usize),
    /// 打开颜色选择器
    TrackColorPickerOpened(usize),
    /// 选择音轨颜色
    TrackColorSelected(usize, Color),
    /// 重置音轨颜色为默认
    TrackColorReset(usize),
    /// 关闭颜色选择器
    TrackColorPickerClosed(usize),
    /// 音轨拖拽排序候选开始（左键按下，用于长按计时与移动跟踪）
    TrackReorderStarted(usize),
    /// 音轨拖拽排序中鼠标移动（列表局部坐标，用于更新插入指示位置）
    TrackReorderMoved {
        /// 列表局部 X 坐标
        x: f32,
        /// 列表局部 Y 坐标
        y: f32,
    },
    /// 音轨拖拽排序结束（携带插入索引；`None` 表示未激活拖拽，不排序）
    TrackReorderEnded(Option<usize>),
    /// 取消音轨拖拽排序（不执行排序，仅清除候选状态）
    TrackReorderCancelled,
}
