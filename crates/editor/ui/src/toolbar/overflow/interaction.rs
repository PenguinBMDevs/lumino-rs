//! 工具栏宽度预算与折叠决策
//!
//! 渲染前拿不到真实布局宽度，所以工具栏按**预估宽度**决定谁让位：
//!
//! 1. 每条可折叠单元的宽度都必须与渲染**同源**（按钮尺寸取 `buttons` 的常量，
//!    条目间距取 [`ToolItem::gap_before`]，渲染循环读同一份）；
//! 2. 预算是**内容区**宽度（外层容器左右各 16px 内边距必须先扣掉），
//!    并且一旦需要折叠，就先把「更多」按钮的位置**预留**出来——
//!    否则内容把最后一个元素顶出可视区，被裁掉的正好是「更多」按钮本身；
//! 3. 折叠顺序由 [`ToolbarGroup::collapse_priority`] 决定；主工具栏（`Tools`）
//!    被展开成 [`ToolItem`] 的**逐项**折叠（从右往左），其余分组整组折叠。
//!
//! 决策是纯函数（宽度 + 模式 → 结构），因此可被单测穷举扫宽度。

use crate::toolbar::Toolbar;
use crate::toolbar::buttons::TOOL_BUTTON_WIDTH;
use crate::toolbar::overflow::state::{ToolItem, ToolbarGroup, ToolbarOverflow};

/// 工具栏外层容器左右内边距合计（`toolbar_view` 的 `.padding([8, 16])`）
///
/// 预算必须扣掉它：`available_width` 是**外层**宽度，内容区比它窄 32px。
pub const TOOLBAR_HORIZONTAL_PADDING: f32 = 32.0;

/// 「更多」按钮左侧的固定间距（px）
///
/// 与 `toolbar_view` 渲染时插入的 `space()` **同源**。
pub const MORE_GAP: f32 = 16.0;

/// 「更多」按钮自身宽度（= 普通工具按钮，见 `buttons::TOOL_BUTTON_WIDTH`）
pub const MORE_BUTTON_WIDTH: f32 = TOOL_BUTTON_WIDTH;

/// 精度选择器与其左侧末项之间的间距（`view/tools.rs` 行内 `space()`）
pub const PRECISION_GAP: f32 = 8.0;

/// 精度选择器宽度估算（`snap_icon 16 + 间隔 6 + 文本 + 左右内边距 16`）
///
/// 文本是唯一变量（预设最长约 `1/16 3`，`Custom 123456` 会更宽）。取 119 使
/// **整组全展开**的宽度正好等于历史预估 568（`377` 条目 + `64` 条目间距 +
/// `8` 精度间距 + `119` 精度选择器 = 568），保证本轮改造不顺手改变
/// "全展开时何时开始折叠"的触发点。
pub const PRECISION_SELECTOR_WIDTH: f32 = 119.0;

/// 工程走带模式下的工具区宽度估算（4 枚固定按钮 + 间距，沿用历史值）
pub const ARRANGEMENT_TOOLS_WIDTH: f32 = 200.0;

/// 一次折叠动作：整组收起 / 主工具栏内的单个条目收起
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollapseStep {
    /// 整组收起
    Group(ToolbarGroup),
    /// 主工具栏内单个条目收起
    Tool(ToolItem),
}

/// 主工具栏（工具区）在给定可见条目下的宽度
///
/// 精度选择器随组常驻：条目全被折叠时它仍在（宽度 = 自身），
/// 只有 `CollapseStep::Group(Tools)` 才会把它一起收走。
pub(crate) fn tools_section_width(items: &[ToolItem], arrangement_mode: bool) -> f32 {
    if arrangement_mode {
        return ARRANGEMENT_TOOLS_WIDTH;
    }

    let mut width = 0.0;
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            width += item.gap_before();
        }
        width += item.width();
    }
    if !items.is_empty() {
        width += PRECISION_GAP;
    }
    width + PRECISION_SELECTOR_WIDTH
}

/// 单个分组的宽度（`Tools` 走逐项计算，其余为整组固定宽度）
fn group_width(group: ToolbarGroup, tools: &[ToolItem], arrangement_mode: bool) -> f32 {
    match group {
        ToolbarGroup::Tools => tools_section_width(tools, arrangement_mode),
        other => fixed_group_width(other),
    }
}

/// 整组折叠分组的固定宽度估算（px）
///
/// 数值与渲染侧固定宽度同源：Record/Playback/Loop/UndoRedo 是容器显式宽度，
/// 右侧两枚开关与仪表盘为本仓历史标定值（略高于实测，宁可多折叠也不溢出）。
fn fixed_group_width(group: ToolbarGroup) -> f32 {
    match group {
        ToolbarGroup::Record => 56.0,
        ToolbarGroup::Playback => 132.0,
        ToolbarGroup::Loop => 40.0,
        ToolbarGroup::UndoRedo => 96.0,
        ToolbarGroup::Dashboard => 201.0,
        ToolbarGroup::OnionSkin => 56.0,
        ToolbarGroup::AutoScroll => 50.0,
        ToolbarGroup::Collaboration => 50.0,
        // 只经 `group_width` 分发；走到这里说明分支写错了
        ToolbarGroup::Tools => 0.0,
    }
}

/// 给定可见分组/条目时的**内容区固定宽度**（组内间距与组间间距均已计入）
///
/// 口径说明：左右两段之间是一个 `Length::Fill` 让位空格（固定宽 0），这里按
/// "多算一次组间距"处理——**故意保守**，宁可提前折叠，也不让最后一枚
/// 「更多」按钮被挤出可视区。
pub(super) fn content_width(
    groups: &[ToolbarGroup],
    tools: &[ToolItem],
    arrangement_mode: bool,
) -> f32 {
    let mut width = 0.0;
    for (index, group) in groups.iter().enumerate() {
        width += group_width(*group, tools, arrangement_mode);
        if index + 1 < groups.len() {
            width += group.spacing_after();
        }
    }
    width
}

impl ToolbarOverflow {
    /// 该折叠结果占用的固定宽度（折叠时已含「更多」按钮预留位）
    ///
    /// 测试用它钉死"更多按钮永远不被裁"：对任意可用宽度都应有
    /// `layout_width <= available_width - TOOLBAR_HORIZONTAL_PADDING`。
    pub fn layout_width(&self, arrangement_mode: bool) -> f32 {
        content_width(&self.visible_groups, &self.visible_tools, arrangement_mode)
            + if self.has_overflow() {
                MORE_GAP + MORE_BUTTON_WIDTH
            } else {
                0.0
            }
    }
}

impl Toolbar {
    /// 计算工具栏折叠结果（宽度不足时按优先级折叠，见模块头）
    ///
    /// 纯函数：同样输入必然同样输出（渲染与溢出菜单各算一次，结果一致）。
    pub fn compute_overflow(
        &self,
        available_width: f32,
        arrangement_mode: bool,
    ) -> ToolbarOverflow {
        // 内容区 = 可用宽度 - 工具栏自身左右内边距
        let content = (available_width - TOOLBAR_HORIZONTAL_PADDING).max(0.0);
        let order = Self::group_order(arrangement_mode);
        let mut visible_tools: Vec<ToolItem> = if arrangement_mode {
            // 走带模式的工具区固定四枚，整组折叠即可（不做逐项）
            Vec::new()
        } else {
            ToolItem::ALL.to_vec()
        };

        // 全部放得下：谁都不折叠，也不需要「更多」按钮
        if content_width(&order, &visible_tools, arrangement_mode) <= content {
            return ToolbarOverflow {
                visible_groups: order,
                hidden_groups: Vec::new(),
                visible_tools,
                hidden_tools: Vec::new(),
            };
        }

        // 需要折叠：先给「更多」按钮留足位置，再按计划逐个收起
        let budget = (content - MORE_GAP - MORE_BUTTON_WIDTH).max(0.0);
        let mut visible_groups = order;
        for step in Self::collapse_plan(arrangement_mode) {
            match step {
                CollapseStep::Tool(item) => {
                    if let Some(index) = visible_tools.iter().position(|visible| *visible == item) {
                        visible_tools.remove(index);
                    }
                }
                CollapseStep::Group(group) => {
                    visible_groups.retain(|visible| *visible != group);
                }
            }
            if content_width(&visible_groups, &visible_tools, arrangement_mode) <= budget {
                break;
            }
        }

        // 隐藏集合按 ORDER / ALL 顺序派生（渲染与菜单都依赖稳定顺序）
        let hidden_groups = Self::group_order(arrangement_mode)
            .into_iter()
            .filter(|group| !visible_groups.contains(group))
            .collect();
        // 走带模式没有"逐项折叠"概念（工具区整组折叠），故隐藏条目恒为空；
        // 否则 14 个条目会被误判成"全被逐项折掉"，菜单里凭空多出工具按钮。
        let hidden_tools = if arrangement_mode {
            Vec::new()
        } else {
            ToolItem::ALL
                .iter()
                .copied()
                .filter(|item| !visible_tools.contains(item))
                .collect()
        };

        ToolbarOverflow {
            visible_groups,
            hidden_groups,
            visible_tools,
            hidden_tools,
        }
    }

    /// 该模式下参与布局的分组顺序（工程走带没有洋葱皮）
    fn group_order(arrangement_mode: bool) -> Vec<ToolbarGroup> {
        ToolbarGroup::ORDER
            .iter()
            .copied()
            .filter(|group| !arrangement_mode || *group != ToolbarGroup::OnionSkin)
            .collect()
    }

    /// 折叠计划（严格按优先级，先到先折叠）
    ///
    /// 主工具栏在计划里被**展开**成 `ToolItem::ALL` 的逆序：从右往左逐个让位，
    /// 最后才是 `Group(Tools)`（连精度选择器一起收起）。
    fn collapse_plan(arrangement_mode: bool) -> Vec<CollapseStep> {
        let mut groups = Self::group_order(arrangement_mode);
        groups.sort_by_key(|group| group.collapse_priority());

        let mut plan = Vec::with_capacity(groups.len() + ToolItem::ALL.len());
        for group in groups {
            if group == ToolbarGroup::Tools && !arrangement_mode {
                plan.extend(
                    ToolItem::ALL
                        .iter()
                        .rev()
                        .map(|item| CollapseStep::Tool(*item)),
                );
            }
            plan.push(CollapseStep::Group(group));
        }
        plan
    }
}
