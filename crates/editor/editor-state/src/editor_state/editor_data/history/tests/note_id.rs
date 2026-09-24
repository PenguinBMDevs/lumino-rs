use super::*;
use crate::EditorData;

/// Bug 回归：接收远端音符（携带真实全局 id）后，本地分配器必须抬到其之上，
/// 否则本地新建音符会复用到对端已占用的 id，造成「跨客户端 id 碰撞」（缺陷 #5）。
#[test]
fn test_ensure_note_id_above_bumps_allocator() {
    let mut data = EditorData::with_f32_notes(0, &[]);
    // 本地分配器从 1 起；插入一个零 id 音符 → 分配 1
    data.insert_note(0, Note::from_raw(0.0, 60, 1.0, 100, 0));
    assert_eq!(
        data.note_id_at(0, 0.0, 60),
        Some(1),
        "首个本地音符应分配到 id=1"
    );

    // 模拟接收远端音符 id=42：抬升本地分配器，避免后续复用到 42
    data.ensure_note_id_above(42);

    // 再插入一个零 id 音符，应分配到 43 而非 1 或 42（无碰撞）
    data.insert_note(0, Note::from_raw(96.0, 62, 1.0, 100, 0));
    let new_id = data.note_id_at(0, 96.0, 62).expect("应找到刚插入的音符");
    assert_eq!(
        new_id, 43,
        "接收远端 id=42 后，本地分配器应抬到 43，避免与对端 id 碰撞"
    );
}

/// 回归：真实绘制路径的 CreateOp 必须记录分配后的真实 id；
/// undo→redo 往返 id 不变（旧实现 CreateOp.note.id=0，redo 会重新分配新 id，
/// 破坏「note id 全局稳定」不变量）。
#[test]
fn test_finish_drawing_captures_id_and_redo_preserves_it() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    let drawn = data
        .finish_drawing(0.0, 60, 80.0, 1.0, 80.0)
        .expect("绘制应成功");
    let real_id = data
        .current_track_notes()
        .get(0)
        .expect("绘制后音符应存在")
        .id;
    assert!(real_id > 0, "绘制必须分配全局唯一 id");
    assert_eq!(drawn.id, real_id, "返回的 Note 应携带真实 id");

    assert!(data.undo());
    assert_eq!(data.current_track_note_count(), 0, "undo 应删除创建音符");

    assert!(data.redo());
    let restored_id = data
        .current_track_notes()
        .get(0)
        .expect("redo 后音符应存在")
        .id;
    assert_eq!(
        restored_id, real_id,
        "redo 必须原样保留 id，不得重新分配（身份稳定）"
    );
}

/// 回归：真实绘制路径 undo/redo 的协作广播必须携带真实 id（旧实现广播 id=0，
/// 对端无法按 id 匹配删除/添加）。
#[test]
fn test_create_undo_redo_collab_sync_uses_real_id() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    let _ = data.finish_drawing(0.0, 60, 80.0, 1.0, 80.0);
    let real_id = data
        .current_track_notes()
        .get(0)
        .expect("绘制后音符应存在")
        .id;

    assert!(data.undo());
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, real_id, "undo 创建广播必须携带真实 id");

    assert!(data.redo());
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, real_id, "redo 创建广播必须携带真实 id");
}
