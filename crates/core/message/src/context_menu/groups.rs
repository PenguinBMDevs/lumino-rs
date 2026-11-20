//! 右键菜单的**单竖列**分组
//!
//! 菜单固定为一列：分组自上而下堆叠，组间画一条分隔线。
//! - **常规组**：剪切 / 复制 / 粘贴 / 删除 / 全选 / 批量编辑 / 形状列表 / 从分享文本粘贴；
//! - **目标组**：只有当前目标可用的动作（音符 = 设刀 / 取消刀；待确认图形 = 形状操作）。
//!
//! 布局只读 [`menu_groups`]；[`super::menu_items`] 保留为扁平序列（旧调用与兼容测试），
//! 两者的一致性由本模块测试锁定。
//!
//! # 为什么常规组不隐藏不适用项
//!
//! 隐藏会让行数与条目位置随目标跳动，用户无法形成肌肉记忆。因此常规组对每个目标
//! 都渲染同样 8 项，不可用的由 [`item_enabled`] 判定并置灰（带 tooltip、不可点击）。
//! 判定分两层：目标是否适用（本模块）与当前编辑状态是否为空操作
//! （[`ContextMenuAvailability`]：无选中音符时的剪切 / 复制 / 删除 / 批量编辑，
//! 无刀源 / 无刀时的两个刀条目）。已 Bake 的图形（DEC-001 只读）是唯一例外：
//! 卡面要求保持 5 项单列不变。
//!
//! # 超高菜单
//!
//! 待确认图形共 27 行（8 常规 + 19 专属），在 720p 可用区内放不下。
//! UI 层把面板高度钳到可用区并在面板内滚动：不拆列、不隐藏、不改变条目顺序。

use super::{ContextMenuAvailability, ContextMenuTarget, PianoRollContextMenuItem as Item};

/// 常规操作组：每个目标都渲染（是否可用见 [`item_enabled`]）
pub const REGULAR_ITEMS: &[Item] = &[
    Item::Cut,
    Item::Copy,
    Item::Paste,
    Item::Delete,
    Item::SelectAll,
    Item::BatchEdit,
    Item::ShapeList,
    Item::PasteShapeText,
];

/// 音符目标的专属组：设刀 / 取消刀（DEBT-17 从常规组移出）
const NOTES_ACTIONS: &[Item] = &[Item::MouldSetKnife, Item::MouldClearKnife];

/// 已 Bake 图形的只读组（DEC-001：不改参数、不重生成音符）
const DRAWN_SHAPE_READ_ONLY: &[Item] = &[
    Item::ShapeList,
    Item::MaskApply,
    Item::CopyShapeText,
    Item::PasteShapeText,
    Item::DeleteDrawnShape,
];

/// 待确认图形的专属组：形状组合 → 遮罩 → 对称/切割 → 刀/饼干模 → 派生
///
/// 删除不在此组：常规组的「删除」与 `DeleteDrawnShape` 走同一条 `DeletePressed`
/// 路径（`handlers/editor_action.rs`），两组都放会出现两个同名按钮。
const PENDING_ACTIONS: &[Item] = &[
    // 形状组合
    Item::LiveShape,
    Item::JoinShapes,
    Item::MergeShapes,
    Item::EditShape,
    // 区域着色 / 遮罩
    Item::PaintAreas,
    Item::MaskToggle,
    Item::MaskSettings,
    // 对称 / 切割
    Item::SymmetryVertical,
    Item::SymmetryHorizontal,
    Item::SliceCut,
    // 刀 / 饼干模
    Item::MouldSetKnife,
    Item::MouldClearKnife,
    Item::MouldShapes,
    // 派生：柔化 / 补间 / 沿路径 / 力度 / 分享 / 退回
    Item::RoundCorners,
    Item::BetweenCreate,
    Item::CopyAlong,
    Item::VelocityUp,
    Item::CopyShapeText,
    Item::ReturnToDrawer,
];

/// 目标对应的菜单分组（单竖列内自上而下，组间画分隔线）
pub const fn menu_groups(target: ContextMenuTarget) -> &'static [&'static [Item]] {
    match target {
        ContextMenuTarget::Notes => &[REGULAR_ITEMS, NOTES_ACTIONS],
        ContextMenuTarget::DrawnShape => &[DRAWN_SHAPE_READ_ONLY],
        ContextMenuTarget::PendingDrawnShape => &[REGULAR_ITEMS, PENDING_ACTIONS],
    }
}

/// 该条目在给定目标与编辑状态下是否可用（不可用 = 置灰 + tooltip + 不响应点击）
///
/// 判据分两层：
/// 1. **目标适用性**（`target_supports`）：条目是否服务于该目标；
/// 2. **状态可用性**（[`ContextMenuAvailability::allows`]）：当前编辑状态是否会
///    让该条目成为空操作（例如无选中音符时的剪切 / 复制 / 删除 / 批量编辑）。
///
/// 目标层优先：目标本就不适用的条目，无论状态如何都保持置灰。
pub const fn item_enabled(
    target: ContextMenuTarget,
    item: Item,
    availability: ContextMenuAvailability,
) -> bool {
    target_supports(target, item) && availability.allows(target, item)
}

/// 条目是否服务于给定目标（不看编辑状态）
const fn target_supports(target: ContextMenuTarget, item: Item) -> bool {
    match target {
        ContextMenuTarget::Notes => matches!(
            item,
            Item::Cut
                | Item::Copy
                | Item::Paste
                | Item::Delete
                | Item::SelectAll
                | Item::BatchEdit
                | Item::ShapeList
                | Item::PasteShapeText
                | Item::MouldSetKnife
                | Item::MouldClearKnife
        ),
        ContextMenuTarget::DrawnShape => matches!(
            item,
            Item::ShapeList
                | Item::MaskApply
                | Item::CopyShapeText
                | Item::PasteShapeText
                | Item::DeleteDrawnShape
        ),
        ContextMenuTarget::PendingDrawnShape => matches!(
            item,
            // 常规组：仅删除与从分享文本粘贴对待确认图形有效
            Item::Delete
                | Item::PasteShapeText
                // 目标专属组
                | Item::LiveShape
                | Item::JoinShapes
                | Item::MergeShapes
                | Item::EditShape
                | Item::PaintAreas
                | Item::MaskToggle
                | Item::MaskSettings
                | Item::SymmetryVertical
                | Item::SymmetryHorizontal
                | Item::SliceCut
                | Item::MouldSetKnife
                | Item::MouldClearKnife
                | Item::MouldShapes
                | Item::RoundCorners
                | Item::BetweenCreate
                | Item::CopyAlong
                | Item::VelocityUp
                | Item::CopyShapeText
                | Item::ReturnToDrawer
        ),
    }
}

/// 置灰项的 tooltip 补充说明（与条目名一起显示）
///
/// 目标不适用时保持通用文案；目标适用但状态不满足时，说明缺什么状态
/// （例如「请先选中音符」「当前没有刀」），避免用户以为功能坏了。
pub fn item_disabled_hint(
    target: ContextMenuTarget,
    item: Item,
    availability: ContextMenuAvailability,
) -> &'static str {
    if target_supports(target, item) {
        availability.disabled_hint(item)
    } else {
        lumino_core::tr!("该操作不适用于当前目标")
    }
}

#[cfg(test)]
mod tests;
