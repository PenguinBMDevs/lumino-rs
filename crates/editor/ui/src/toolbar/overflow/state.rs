//! 溢出菜单类型定义
//!
//! - [`ToolbarGroup`]：可折叠的工具栏分组（整组折叠）；
//! - [`ToolItem`]：**主工具栏内部**可逐项折叠的条目（铅笔/橡皮等）；
//! - [`ToolbarOverflow`]：一次宽度预算计算出的可见/隐藏结果；
//! - [`OverflowMenuItem`]：溢出菜单中的单个按钮项。

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
    /// 洋葱皮全局显示开关（其它音轨的幽灵音符；右侧，默认关闭）
    OnionSkin,
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
        ToolbarGroup::OnionSkin,
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
    ///
    /// 洋葱皮开关排在最左：它属于"卷帘显示选项"，与自动滚动同族；
    /// 协作是应用级入口，固定压在最右。
    pub const RIGHT: &[ToolbarGroup] = &[
        ToolbarGroup::OnionSkin,
        ToolbarGroup::AutoScroll,
        ToolbarGroup::Collaboration,
    ];

    /// 分组收起优先级（数字越小越先被折叠，同值按 [`Self::ORDER`] 先后）
    ///
    /// 产品口径（**顺序是需求，不是实现细节**）：
    /// 1. 右侧显示开关：协作 → 洋葱皮 → 自动滚动；
    /// 2. **主工具栏内部的工具按钮逐个让位**（`ToolItem` 逐项折叠，见
    ///    `interaction::collapse_plan`）；
    /// 3. 主工具栏整组（连同精度选择器）整体收起；
    /// 4. 检测仪表盘——**整组**折叠，且**必须晚于**主工具栏；
    /// 5. 循环 → 撤销/重做 → 播放 → 录制（越靠左越晚折叠）。
    ///
    /// ⚠️ `Tools` 必须排在 `Dashboard` 之前：仪表盘不得抢在工具条前面消失。
    /// 仪表盘是**整组**折叠——它内部是双列时间码，没有可读的"逐个折叠"中间态。
    pub fn collapse_priority(self) -> usize {
        match self {
            ToolbarGroup::Collaboration => 0,
            // 与 AutoScroll 同级：两个都是"显示开关"。同值时稳定排序按 ORDER 先后决定，
            // 洋葱皮在前，故先被折叠——新功能让位给既有自动滚动。
            ToolbarGroup::OnionSkin | ToolbarGroup::AutoScroll => 1,
            ToolbarGroup::Tools => 2,
            ToolbarGroup::Dashboard => 3,
            ToolbarGroup::Loop => 4,
            ToolbarGroup::UndoRedo => 5,
            ToolbarGroup::Playback => 6,
            ToolbarGroup::Record => 7,
        }
    }

    /// 分组之间的间距（与 `toolbar_view.rs` 中 row 的显式 `space()` 对应）
    pub fn spacing_after(self) -> f32 {
        match self {
            ToolbarGroup::Record => 4.0,
            ToolbarGroup::Playback => 8.0,
            ToolbarGroup::Loop => 8.0,
            ToolbarGroup::UndoRedo => 16.0,
            ToolbarGroup::Dashboard => 16.0,
            ToolbarGroup::Tools => 0.0,
            ToolbarGroup::OnionSkin => 16.0,
            ToolbarGroup::AutoScroll => 16.0,
            ToolbarGroup::Collaboration => 0.0,
        }
    }
}

/// 主工具栏（铅笔/橡皮等所在的那一排）内**可逐项折叠**的条目
///
/// 顺序即视觉顺序（见 [`Self::ALL`]）。折叠时**从右往左**逐项剥离——这就是
/// "按顺序一个一个折叠"的口径：越靠右的次要动作越先让位，指针/铅笔/橡皮留到最后。
///
/// 精度选择器**不在**本枚举内：它不是"工具按钮"，而是工具区的锚点，随组常驻
/// （只有整组折叠时才一起消失），避免"精度突然没了"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolItem {
    /// 指针工具
    Pointer,
    /// Y 向框选
    PointerYSelect,
    /// 铅笔
    Pencil,
    /// 橡皮擦
    Eraser,
    /// 音符画工具箱开关
    DrawToolbox,
    /// 量化
    Quantize,
    /// 变速
    Speed,
    /// 垂直翻转
    FlipVertical,
    /// 水平翻转
    FlipHorizontal,
    /// 分割
    Split,
    /// 合并
    Glue,
    /// 移调 -1
    TransposeDown,
    /// 移调 +1
    TransposeUp,
    /// 连奏
    Tie,
}

impl ToolItem {
    /// 视觉顺序（与 `view/tools.rs` 的渲染顺序**同源**：渲染也按本顺序取条目）
    pub const ALL: &[ToolItem] = &[
        ToolItem::Pointer,
        ToolItem::PointerYSelect,
        ToolItem::Pencil,
        ToolItem::Eraser,
        ToolItem::DrawToolbox,
        ToolItem::Quantize,
        ToolItem::Speed,
        ToolItem::FlipVertical,
        ToolItem::FlipHorizontal,
        ToolItem::Split,
        ToolItem::Glue,
        ToolItem::TransposeDown,
        ToolItem::TransposeUp,
        ToolItem::Tie,
    ];

    /// 条目自身宽度（px）
    ///
    /// 与 `buttons` 工厂的尺寸契约同源：选择器走 [`TOOL_SELECTOR_WIDTH`]，
    /// 其余按钮走 [`TOOL_BUTTON_WIDTH`]。**不得**在这里写魔数。
    pub fn width(self) -> f32 {
        use crate::toolbar::buttons::{TOOL_BUTTON_WIDTH, TOOL_SELECTOR_WIDTH};
        match self {
            ToolItem::Pointer
            | ToolItem::PointerYSelect
            | ToolItem::Pencil
            | ToolItem::Eraser
            | ToolItem::DrawToolbox => TOOL_SELECTOR_WIDTH,
            _ => TOOL_BUTTON_WIDTH,
        }
    }

    /// 条目左侧的固定间距（行内 `space()` 宽度；首项为 0）
    ///
    /// 与 `view/tools.rs` 同源：渲染循环直接用本方法建间隔，预算也用它——
    /// 一处改动两边同步，不会出现"估的比画的多/少"。
    pub fn gap_before(self) -> f32 {
        match self {
            ToolItem::Pointer => 0.0,
            // 语义分组之间留双倍间距（翻转组之后 / 移调 / 连奏）
            ToolItem::Split | ToolItem::TransposeDown | ToolItem::Tie => 8.0,
            _ => 4.0,
        }
    }
}

/// 一次宽度预算计算出的折叠结果
///
/// 只描述"谁可见"，不关心如何渲染；`view` 与 `overflow::view` 各自消费。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolbarOverflow {
    /// 可见分组（保持 [`ToolbarGroup::ORDER`] 顺序）
    pub visible_groups: Vec<ToolbarGroup>,
    /// 被折叠的分组（保持 ORDER 顺序）
    pub hidden_groups: Vec<ToolbarGroup>,
    /// 主工具栏中仍可见的条目（保持 [`ToolItem::ALL`] 顺序，必为其前缀）
    pub visible_tools: Vec<ToolItem>,
    /// 主工具栏中被折叠的条目（保持 ALL 顺序，必为其后缀）
    pub hidden_tools: Vec<ToolItem>,
}

impl ToolbarOverflow {
    /// 是否有任何内容被折叠（= 需要显示「更多」按钮）
    pub fn has_overflow(&self) -> bool {
        !self.hidden_groups.is_empty() || !self.hidden_tools.is_empty()
    }

    /// 主工具栏是否整组收起（精度选择器也随之消失）
    pub fn tools_group_hidden(&self) -> bool {
        self.hidden_groups.contains(&ToolbarGroup::Tools)
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
