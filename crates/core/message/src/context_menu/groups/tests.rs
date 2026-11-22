//! 右键菜单分组与可用性的单元测试
//!
//! 从 `groups.rs` 拆出（文件长度纪律）；`super::*` 仍指向 `context_menu::groups`。

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

/// 「所有状态都满足」：用于验证目标维度，不引入状态干扰
fn full() -> ContextMenuAvailability {
    ContextMenuAvailability {
        has_note_selection: true,
        has_knife_source: true,
        has_knife: true,
    }
}

#[test]
fn test_notes_regular_group_is_fully_enabled() {
    for item in REGULAR_ITEMS {
        assert!(
            item_enabled(ContextMenuTarget::Notes, *item, full()),
            "音符目标的常规项在状态满足时必须全可用：{item:?}"
        );
    }
    assert!(item_enabled(
        ContextMenuTarget::Notes,
        Item::MouldSetKnife,
        full()
    ));
    assert!(!item_enabled(
        ContextMenuTarget::Notes,
        Item::RoundCorners,
        full()
    ));
}

#[test]
fn test_notes_selection_items_follow_state() {
    let empty = ContextMenuAvailability::default();
    for item in [
        Item::Cut,
        Item::Copy,
        Item::Delete,
        Item::BatchEdit,
        Item::MouldSetKnife,
        Item::MouldClearKnife,
    ] {
        assert!(
            !item_enabled(ContextMenuTarget::Notes, item, empty),
            "无状态时 {item:?} 必须置灰"
        );
    }
    // 与选区无关的条目不能被误伤：粘贴 / 全选 / 形状列表 / 分享文本粘贴
    for item in [
        Item::Paste,
        Item::SelectAll,
        Item::ShapeList,
        Item::PasteShapeText,
    ] {
        assert!(
            item_enabled(ContextMenuTarget::Notes, item, empty),
            "{item:?} 不依赖音符选区，必须保持可用"
        );
    }
}

#[test]
fn test_target_mismatch_wins_over_state() {
    assert!(!item_enabled(
        ContextMenuTarget::DrawnShape,
        Item::Delete,
        full()
    ));
    assert!(!item_enabled(
        ContextMenuTarget::DrawnShape,
        Item::BatchEdit,
        full()
    ));
}

#[test]
fn test_pending_regular_group_greys_notes_only_items() {
    let greys: Vec<Item> = REGULAR_ITEMS
        .iter()
        .copied()
        .filter(|item| !item_enabled(ContextMenuTarget::PendingDrawnShape, *item, full()))
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
        Item::Delete,
        full()
    ));
    assert!(item_enabled(
        ContextMenuTarget::PendingDrawnShape,
        Item::PasteShapeText,
        full()
    ));
}

#[test]
fn test_drawn_shape_enables_read_only_set_only() {
    for item in DRAWN_SHAPE_READ_ONLY {
        assert!(item_enabled(ContextMenuTarget::DrawnShape, *item, full()));
    }
    assert!(!item_enabled(
        ContextMenuTarget::DrawnShape,
        Item::EditShape,
        full()
    ));
    assert!(!item_enabled(
        ContextMenuTarget::DrawnShape,
        Item::Delete,
        full()
    ));
    assert!(!item_enabled(
        ContextMenuTarget::DrawnShape,
        Item::RoundCorners,
        full()
    ));
}

#[test]
fn test_disabled_hint_is_translated() {
    let empty = ContextMenuAvailability::default();
    assert!(!item_disabled_hint(ContextMenuTarget::Notes, Item::Cut, empty).is_empty());
    assert!(!item_disabled_hint(ContextMenuTarget::DrawnShape, Item::EditShape, empty).is_empty());
}
