//! 溢出菜单类型定义
//!
//! 定义工具栏分组标识（`ToolbarGroup`）和溢出菜单项（`OverflowMenuItem`）。

use crate::Message;

/// 可折叠工具栏分组标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarGroup {
    /// 录制按钮
    Record,
    /// 播放控制（快退/播放/暂停/快进）
    Playback,
    /// 循环播放切换
    Loop,
    /// 撤销/重做
    UndoRedo,
    /// 检测仪表盘（性能/时间码显示）
    Dashboard,
    /// 工具选择区（指针/铅笔/橡皮/曲线/量化/变速/翻转/分割/合并/移调/连奏/精度）
    Tools,
    /// 自动滚动模式
    AutoScroll,
    /// 协作按钮
    Collaboration,
}

impl ToolbarGroup {
    /// 分组在工具栏中的显示顺序
    pub const ORDER: &[ToolbarGroup] = &[
        ToolbarGroup::Record,
        ToolbarGroup::Playback,
        ToolbarGroup::Loop,
        ToolbarGroup::UndoRedo,
        ToolbarGroup::Dashboard,
        ToolbarGroup::Tools,
        ToolbarGroup::AutoScroll,
        ToolbarGroup::Collaboration,
    ];

    /// 左侧分组（靠左排列）
    pub const LEFT: &[ToolbarGroup] = &[
        ToolbarGroup::Record,
        ToolbarGroup::Playback,
        ToolbarGroup::Loop,
        ToolbarGroup::UndoRedo,
        ToolbarGroup::Dashboard,
        ToolbarGroup::Tools,
    ];

    /// 右侧分组（靠右排列）
    pub const RIGHT: &[ToolbarGroup] = &[ToolbarGroup::AutoScroll, ToolbarGroup::Collaboration];

    /// 分组收起优先级（数字越小越优先被折叠）
    pub fn collapse_priority(self) -> usize {
        match self {
            ToolbarGroup::Collaboration => 0,
            ToolbarGroup::AutoScroll => 1,
            ToolbarGroup::Dashboard => 2,
            ToolbarGroup::Tools => 3,
            ToolbarGroup::Loop => 4,
            ToolbarGroup::UndoRedo => 5,
            ToolbarGroup::Playback => 6,
            ToolbarGroup::Record => 7,
        }
    }

    /// 分组在工具栏中的预估宽度（px）
    pub fn width(self) -> f32 {
        match self {
            ToolbarGroup::Record => 56.0,
            ToolbarGroup::Playback => 132.0,
            ToolbarGroup::Loop => 40.0,
            ToolbarGroup::UndoRedo => 64.0,
            ToolbarGroup::Dashboard => 201.0,
            ToolbarGroup::Tools => 568.0,
            ToolbarGroup::AutoScroll => 50.0,
            ToolbarGroup::Collaboration => 50.0,
        }
    }

    /// 分组之间的间距（与 `toolbar_view.rs` 中 row! 宏的 spacing 对应）
    pub fn spacing_after(self) -> f32 {
        match self {
            ToolbarGroup::Record => 4.0,
            ToolbarGroup::Playback => 8.0,
            ToolbarGroup::Loop => 8.0,
            ToolbarGroup::UndoRedo => 16.0,
            ToolbarGroup::Dashboard => 16.0,
            ToolbarGroup::Tools => 0.0,
            ToolbarGroup::AutoScroll => 16.0,
            ToolbarGroup::Collaboration => 0.0,
        }
    }
}

/// 溢出菜单中的单个按钮项
pub struct OverflowMenuItem {
    /// 图标
    pub icon: crate::resources::icon::Icon,
    /// 悬浮提示
    pub tooltip: &'static str,
    /// 点击消息
    pub on_press: Message,
    /// 是否禁用（需要选中音符的工具在无选中时置灰）
    pub enabled: bool,
}
