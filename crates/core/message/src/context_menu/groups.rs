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
//! 已 Bake 的图形（DEC-001 只读）是唯一例外：卡面要求保持 5 项单列不变。
//!
//! # 超高菜单
//!
//! 待确认图形共 27 行（8 常规 + 19 专属），在 720p 可用区内放不下。
//! UI 层把面板高度钳到可用区并在面板内滚动：不拆列、不隐藏、不改变条目顺序。

use super::{ContextMenuTarget, PianoRollContextMenuItem as Item};

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

/// 该条目在给定目标下是否可用（不可用 = 置灰 + tooltip + 不响应点击）
///
/// 判据是**生产侧真实行为**，不是条目出现在哪一组：
/// - 剪切 / 复制 / 粘贴 / 全选 / 批量编辑只作用于音符（`EditorAction` 的 Notes 路径）；
/// - 形状列表读 ShapeStore 模型，待确认镜像没有模型（DRAW-041C）；
/// - 删除与从分享文本粘贴对待确认图形同样有效（前者走 `DeletePressed`）。
pub const fn item_enabled(target: ContextMenuTarget, item: Item) -> bool {
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
pub fn item_disabled_hint() -> &'static str {
    lumino_core::tr!("该操作不适用于当前目标")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGETS: [ContextMenuTarget; 3] = [
        ContextMenuTarget::Notes,
        ContextMenuTarget::DrawnShape,
        ContextMenuTarget::PendingDrawnShape,
    ];

    fn flat_rows(target: ContextMenuTarget) -> Vec<Item> {
        menu_groups(target)
            .iter()
            .flat_map(|group| group.iter().copied())
            .collect()
    }

    #[test]
    fn test_groups_have_no_duplicate_items() {
        for target in TARGETS {
            let mut seen = Vec::new();
            for group in menu_groups(target) {
                assert!(!group.is_empty(), "{target:?} 不得有空气组");
                for item in *group {
                    assert!(
                        !seen.contains(item),
                        "{target:?} 的菜单分组重复条目：{item:?}"
                    );
                    seen.push(*item);
                }
            }
        }
    }

    #[test]
    fn test_row_counts_are_stable() {
        // 行数即单列高度：音符 10 行、已 Bake 5 行、待确认 27 行。
        assert_eq!(flat_rows(ContextMenuTarget::Notes).len(), 10);
        assert_eq!(flat_rows(ContextMenuTarget::DrawnShape).len(), 5);
        assert_eq!(flat_rows(ContextMenuTarget::PendingDrawnShape).len(), 27);
    }

    #[test]
    fn test_every_group_item_exists_in_flat_table() {
        // `menu_items` 是改造前的扁平表：待确认目标本来就只有专属动作，
        // 常规组（剪切 / 复制 / …）对它来说是新增的结构，因此允许取自 `REGULAR_ITEMS`。
        for target in TARGETS {
            let flat = super::super::menu_items(target);
            for group in menu_groups(target) {
                for item in *group {
                    assert!(
                        flat.contains(item) || REGULAR_ITEMS.contains(item),
                        "{target:?} 的分组里有扁平表与常规表都未登记的条目：{item:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_regular_group_is_the_same_for_notes_and_pending() {
        assert_eq!(menu_groups(ContextMenuTarget::Notes)[0], REGULAR_ITEMS);
        assert_eq!(
            menu_groups(ContextMenuTarget::PendingDrawnShape)[0],
            REGULAR_ITEMS,
            "常规组必须结构稳定（不适用项置灰而非隐藏）"
        );
    }

    #[test]
    fn test_notes_second_group_is_knife_only() {
        let groups = menu_groups(ContextMenuTarget::Notes);
        assert_eq!(groups.len(), 2, "音符目标：常规组 + 设刀组");
        assert_eq!(groups[1], NOTES_ACTIONS);
    }

    #[test]
    fn test_drawn_shape_stays_single_group() {
        let groups = menu_groups(ContextMenuTarget::DrawnShape);
        assert_eq!(groups.len(), 1, "已 Bake 图形保持单组");
        assert_eq!(groups[0], DRAWN_SHAPE_READ_ONLY);
    }

    #[test]
    fn test_pending_groups_cover_all_actions_except_duplicate_delete() {
        let rows = flat_rows(ContextMenuTarget::PendingDrawnShape);
        for item in super::super::menu_items(ContextMenuTarget::PendingDrawnShape) {
            if *item == Item::DeleteDrawnShape {
                assert!(
                    !rows.contains(item),
                    "删除只保留常规组一处（两条同义路径不重复渲染）"
                );
                continue;
            }
            assert!(rows.contains(item), "待确认图形缺少条目：{item:?}");
        }
    }

    #[test]
    fn test_notes_regular_group_is_fully_enabled() {
        for item in REGULAR_ITEMS {
            assert!(
                item_enabled(ContextMenuTarget::Notes, *item),
                "音符目标的常规项必须全可用：{item:?}"
            );
        }
        assert!(item_enabled(ContextMenuTarget::Notes, Item::MouldSetKnife));
        assert!(!item_enabled(ContextMenuTarget::Notes, Item::RoundCorners));
    }

    #[test]
    fn test_pending_regular_group_greys_notes_only_items() {
        let greys: Vec<Item> = REGULAR_ITEMS
            .iter()
            .copied()
            .filter(|item| !item_enabled(ContextMenuTarget::PendingDrawnShape, *item))
            .collect();
        assert_eq!(
            greys,
            vec![
                Item::Cut,
                Item::Copy,
                Item::Paste,
                Item::SelectAll,
                Item::BatchEdit,
                Item::ShapeList,
            ],
            "只置灰真正的音符专属动作"
        );
        assert!(item_enabled(
            ContextMenuTarget::PendingDrawnShape,
            Item::Delete
        ));
        assert!(item_enabled(
            ContextMenuTarget::PendingDrawnShape,
            Item::PasteShapeText
        ));
    }

    #[test]
    fn test_drawn_shape_enables_read_only_set_only() {
        for item in DRAWN_SHAPE_READ_ONLY {
            assert!(item_enabled(ContextMenuTarget::DrawnShape, *item));
        }
        assert!(!item_enabled(
            ContextMenuTarget::DrawnShape,
            Item::EditShape
        ));
        assert!(!item_enabled(ContextMenuTarget::DrawnShape, Item::Delete));
        assert!(!item_enabled(
            ContextMenuTarget::DrawnShape,
            Item::RoundCorners
        ));
    }

    #[test]
    fn test_disabled_hint_is_translated() {
        assert!(!item_disabled_hint().is_empty());
    }
}
