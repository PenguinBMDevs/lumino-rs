//! 历史步名投影测试（DRAW-035B）
//!
//! `history_kind` 是 `OpKind → HistoryKind` 的唯一映射点：形状编辑必须
//! 投影为 `ShapeEdit`，音符类步名不得回归。

use super::history_kind;
use crate::state::root_state::HistoryKind;
use lumino_note_core::history::OpKind;

#[test]
fn test_shape_edit_maps_to_shape_edit() {
    assert_eq!(history_kind(OpKind::ShapeEdit), HistoryKind::ShapeEdit);
}

#[test]
fn test_note_kinds_do_not_regress() {
    assert_eq!(history_kind(OpKind::NoteCreate), HistoryKind::NoteCreate);
    assert_eq!(history_kind(OpKind::NoteMove), HistoryKind::NoteMove);
    assert_eq!(history_kind(OpKind::NoteDelete), HistoryKind::NoteDelete);
    assert_eq!(
        history_kind(OpKind::NoteTransform),
        HistoryKind::NoteTransform
    );
    assert_eq!(
        history_kind(OpKind::VelocityEdit),
        HistoryKind::VelocityEdit
    );
    assert_eq!(
        history_kind(OpKind::AutomationEdit),
        HistoryKind::AutomationEdit
    );
    assert_eq!(history_kind(OpKind::Recording), HistoryKind::Recording);
    assert_eq!(history_kind(OpKind::Other), HistoryKind::Other);
    assert_eq!(history_kind(OpKind::ChainMarker), HistoryKind::Other);
}
