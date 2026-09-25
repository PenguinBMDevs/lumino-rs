//! EditorData 历史记录与 MoveOp 单元测试
//!
//! 覆盖：
//! - `apply_move_ops` 正向/反向应用
//! - key clamp、跨轨操作（document 轨道构造时固定）
//! - `move_ops_from_drag_state` 连续区间拆分、delta 饱和
//! - 基于 MoveOp 的 undo/redo 往返

use super::*;
use lumino_note_core::note::Note;

mod collab_sync;
mod move_ops;
mod note_id;
mod track_order;
mod transform_collab_sync;

pub(crate) fn make_data_with_notes() -> EditorData {
    EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    )
}
