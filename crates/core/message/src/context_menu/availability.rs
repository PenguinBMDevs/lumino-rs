//! 右键菜单条目的**状态可用性**
//!
//! 目标（[`ContextMenuTarget`]）只决定条目集；同一个条目在不同编辑状态下
//! 也可能是空操作（例如没有选中音符时的剪切 / 复制 / 删除 / 批量编辑）。
//! 本模块把“状态是否允许”从分组表里拆出来，供渲染与键盘确认共用同一判据。

use super::{ContextMenuTarget, PianoRollContextMenuItem as Item};

/// 打开右键菜单时的编辑状态快照（全部为“是否存在”布尔量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextMenuAvailability {
    /// 当前有选中的音符
    pub has_note_selection: bool,
    /// 有可作刀的内容：选中音符或选中待确认形状
    pub has_knife_source: bool,
    /// 当前已设刀
    pub has_knife: bool,
}

impl ContextMenuAvailability {
    /// 目标适用之后，状态是否允许该条目
    ///
    /// 这里只回答“状态”维度；目标维度由 [`super::item_enabled`] 先行判定。
    pub const fn allows(self, target: ContextMenuTarget, item: Item) -> bool {
        match (target, item) {
            (ContextMenuTarget::Notes, Item::Cut | Item::Copy | Item::Delete | Item::BatchEdit) => {
                self.has_note_selection
            }
            (_, Item::MouldSetKnife) => self.has_knife_source,
            (_, Item::MouldClearKnife) => self.has_knife,
            _ => true,
        }
    }

    /// 置灰原因（仅在「目标适用、状态不允许」时调用）
    pub fn disabled_hint(self, item: Item) -> &'static str {
        match item {
            Item::Cut | Item::Copy | Item::Delete | Item::BatchEdit => {
                lumino_core::tr!("请先选中音符")
            }
            Item::MouldSetKnife => lumino_core::tr!("请先选中音符或待确认形状"),
            Item::MouldClearKnife => lumino_core::tr!("当前没有刀"),
            _ => lumino_core::tr!("该操作不适用于当前目标"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_items() -> [Item; 4] {
        [Item::Cut, Item::Copy, Item::Delete, Item::BatchEdit]
    }

    #[test]
    fn test_note_actions_require_note_selection() {
        let none = ContextMenuAvailability::default();
        for item in note_items() {
            assert!(
                !none.allows(ContextMenuTarget::Notes, item),
                "无选中音符时 {item:?} 必须不可用"
            );
        }

        let selected = ContextMenuAvailability {
            has_note_selection: true,
            ..ContextMenuAvailability::default()
        };
        for item in note_items() {
            assert!(
                selected.allows(ContextMenuTarget::Notes, item),
                "有选中音符时 {item:?} 必须可用"
            );
        }
    }

    #[test]
    fn test_set_knife_requires_knife_source() {
        let none = ContextMenuAvailability::default();
        assert!(!none.allows(ContextMenuTarget::Notes, Item::MouldSetKnife));
        assert!(!none.allows(ContextMenuTarget::PendingDrawnShape, Item::MouldSetKnife));

        let source = ContextMenuAvailability {
            has_knife_source: true,
            ..ContextMenuAvailability::default()
        };
        assert!(source.allows(ContextMenuTarget::Notes, Item::MouldSetKnife));
        assert!(source.allows(ContextMenuTarget::PendingDrawnShape, Item::MouldSetKnife));
    }

    #[test]
    fn test_clear_knife_requires_knife() {
        let none = ContextMenuAvailability::default();
        assert!(!none.allows(ContextMenuTarget::Notes, Item::MouldClearKnife));

        let knife = ContextMenuAvailability {
            has_knife: true,
            ..ContextMenuAvailability::default()
        };
        assert!(knife.allows(ContextMenuTarget::Notes, Item::MouldClearKnife));
        assert!(knife.allows(ContextMenuTarget::PendingDrawnShape, Item::MouldClearKnife));
    }

    #[test]
    fn test_unrelated_items_ignore_state() {
        let none = ContextMenuAvailability::default();
        assert!(none.allows(ContextMenuTarget::Notes, Item::Paste));
        assert!(none.allows(ContextMenuTarget::Notes, Item::SelectAll));
        assert!(none.allows(ContextMenuTarget::Notes, Item::ShapeList));
        assert!(none.allows(ContextMenuTarget::Notes, Item::PasteShapeText));
    }

    #[test]
    fn test_disabled_hint_names_the_missing_state() {
        let none = ContextMenuAvailability::default();
        assert_eq!(
            none.disabled_hint(Item::Cut),
            lumino_core::tr!("请先选中音符")
        );
        assert_eq!(
            none.disabled_hint(Item::MouldSetKnife),
            lumino_core::tr!("请先选中音符或待确认形状")
        );
        assert_eq!(
            none.disabled_hint(Item::MouldClearKnife),
            lumino_core::tr!("当前没有刀")
        );
    }
}
