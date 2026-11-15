//! 右键菜单的**列**布局（DRAW-046）
//!
//! 菜单条目分两类：
//! - **常规列**：剪切 / 复制 / 粘贴 / 删除 / 全选 / 批量编辑 / 形状列表 / 从分享文本粘贴；
//! - **目标专属列**：只有当前目标可用的动作（音符 = 设刀 / 取消刀；待确认图形 = 形状操作）。
//!
//! 布局只读 [`menu_columns`]；[`super::menu_items`] 保留为扁平序列（旧调用与兼容测试），
//! 两者的一致性由本模块测试锁定。
//!
//! # 为什么第一列不隐藏不适用项
//!
//! 隐藏会让列高与按钮位置随目标跳动，用户无法形成肌肉记忆。因此常规列对每个目标
//! 都渲染同样 8 项，不可用的由 [`item_enabled`] 判定并置灰（带 tooltip、不可点击）。
//! 已 Bake 的图形（DEC-001 只读）是唯一例外：卡面要求保持 5 项单列不变。

use super::{ContextMenuTarget, PianoRollContextMenuItem as Item};

/// 常规操作列：每个目标都渲染（是否可用见 [`item_enabled`]）
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

/// 音符目标的专属列：设刀 / 取消刀（DEBT-17 从常规列移出）
const NOTES_ACTIONS: &[Item] = &[Item::MouldSetKnife, Item::MouldClearKnife];

/// 已 Bake 图形的只读列（DEC-001：不改参数、不重生成音符）
const DRAWN_SHAPE_READ_ONLY: &[Item] = &[
    Item::ShapeList,
    Item::MaskApply,
    Item::CopyShapeText,
    Item::PasteShapeText,
    Item::DeleteDrawnShape,
];

/// 待确认图形的专属列：形状组合 → 遮罩 → 对称/切割 → 刀/饼干模 → 派生
///
/// 行式菜单（图标 + 文字）没有 8 行上限，这 19 项可以放进同一列，
/// 与常规列一起正好两列。
///
/// 删除不在此列：常规列的「删除」与 `DeleteDrawnShape` 走同一条 `DeletePressed`
/// 路径（`handlers/editor_action.rs`），两列都放会出现两个同名按钮。
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

/// 单列行数预算：24px 行高 + 上下内边距 16px → 20 行 = 496px，仍低于 500px 上限。
///
/// 行式菜单没有「每列 8 行」的硬限制；本常量只用于把面板高度锁在可用区内，
/// 由 `lumino-ui-egui` 的几何测试校验。
pub const MAX_ROWS_PER_COLUMN: usize = 20;

/// 目标对应的菜单列（列内自上而下，列间自左向右）
///
/// 行高 24：常规 8 行；待确认图形 19 行（472px）仍在 720p 可用区内。
pub const fn menu_columns(target: ContextMenuTarget) -> &'static [&'static [Item]] {
    match target {
        ContextMenuTarget::Notes => &[REGULAR_ITEMS, NOTES_ACTIONS],
        ContextMenuTarget::DrawnShape => &[DRAWN_SHAPE_READ_ONLY],
        ContextMenuTarget::PendingDrawnShape => &[REGULAR_ITEMS, PENDING_ACTIONS],
    }
}

/// 该条目在给定目标下是否可用（不可用 = 置灰 + tooltip + 不响应点击）
///
/// 判据是**生产侧真实行为**，不是条目出现在哪一列：
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
            // 常规列：仅删除与从分享文本粘贴对待确认图形有效
            Item::Delete
                | Item::PasteShapeText
                // 目标专属列
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

    #[test]
    fn test_columns_have_no_duplicate_items() {
        for target in TARGETS {
            let mut seen = Vec::new();
            for column in menu_columns(target) {
                assert!(!column.is_empty(), "{target:?} 不得有空气列");
                for item in *column {
                    assert!(
                        !seen.contains(item),
                        "{target:?} 的菜单列重复条目：{item:?}"
                    );
                    seen.push(*item);
                }
            }
        }
    }

    #[test]
    fn test_columns_stay_within_row_budget() {
        for target in TARGETS {
            for column in menu_columns(target) {
                assert!(
                    column.len() <= MAX_ROWS_PER_COLUMN,
                    "{target:?} 出现 {} 行的列（预算 {MAX_ROWS_PER_COLUMN}）",
                    column.len()
                );
            }
        }
    }

    #[test]
    fn test_every_column_item_exists_in_flat_table() {
        // `menu_items` 是改造前的扁平表：待确认目标本来就只有专属动作，
        // 常规列（剪切 / 复制 / …）对它来说是新增的结构，因此允许取自 `REGULAR_ITEMS`。
        for target in TARGETS {
            let flat = super::super::menu_items(target);
            for column in menu_columns(target) {
                for item in *column {
                    assert!(
                        flat.contains(item) || REGULAR_ITEMS.contains(item),
                        "{target:?} 的列里有扁平表与常规表都未登记的条目：{item:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_regular_column_is_the_same_for_notes_and_pending() {
        assert_eq!(menu_columns(ContextMenuTarget::Notes)[0], REGULAR_ITEMS);
        assert_eq!(
            menu_columns(ContextMenuTarget::PendingDrawnShape)[0],
            REGULAR_ITEMS,
            "常规列必须结构稳定（不适用项置灰而非隐藏）"
        );
    }

    #[test]
    fn test_notes_second_column_is_knife_only() {
        let columns = menu_columns(ContextMenuTarget::Notes);
        assert_eq!(columns.len(), 2, "音符目标：常规列 + 设刀列");
        assert_eq!(columns[1], NOTES_ACTIONS);
    }

    #[test]
    fn test_drawn_shape_stays_single_column() {
        let columns = menu_columns(ContextMenuTarget::DrawnShape);
        assert_eq!(columns.len(), 1, "已 Bake 图形保持单列");
        assert_eq!(columns[0], DRAWN_SHAPE_READ_ONLY);
    }

    #[test]
    fn test_pending_columns_cover_all_actions_except_duplicate_delete() {
        let columns = menu_columns(ContextMenuTarget::PendingDrawnShape);
        let columns_flat: Vec<Item> = columns.iter().flat_map(|c| c.iter().copied()).collect();
        for item in super::super::menu_items(ContextMenuTarget::PendingDrawnShape) {
            if *item == Item::DeleteDrawnShape {
                assert!(
                    !columns_flat.contains(item),
                    "删除只保留常规列一处（两条同义路径不重复渲染）"
                );
                continue;
            }
            assert!(columns_flat.contains(item), "待确认图形缺少条目：{item:?}");
        }
    }

    #[test]
    fn test_notes_regular_column_is_fully_enabled() {
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
    fn test_pending_regular_column_greys_notes_only_items() {
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
