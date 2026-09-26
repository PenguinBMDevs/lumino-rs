use super::*;
use crate::EditorData;

/// 去 ID 回归：音符身份即其音乐内容（按值引用）。
/// 本文件原先覆盖全局 u64 分配器（抬水位、绘制分配 id、redo 保留 id），
/// 去 ID 后逐项改为按值断言：插入/绘制/undo-redo 均以
/// `(tick, key, length)` 稳定性为准，经 `position_of` 窗口定位（无全扫）。
#[test]
fn test_value_identity_stable_across_insert() {
    let mut data = EditorData::with_f32_notes(0, &[]);
    // 插入两个不同值音符 → 各自按值可定位
    assert!(data.insert_note(0, Note::from_raw(0.0, 60, 1.0, 100, 0)));
    assert!(data.insert_note(0, Note::from_raw(96.0, 62, 1.0, 100, 0)));
    assert_eq!(data.track_notes(0).len(), 2);
    let track = data.track_notes(0);
    let first = super::super::super::accessors::note_to_event(Note::from_raw(0.0, 60, 1.0, 100, 0));
    let second =
        super::super::super::accessors::note_to_event(Note::from_raw(96.0, 62, 1.0, 100, 0));
    assert!(
        track.position_of(&first).is_some(),
        "首个插入音符应按值可定位"
    );
    assert!(
        track.position_of(&second).is_some(),
        "第二个插入音符应按值可定位"
    );
}

/// 回归：真实绘制路径的 CreateOp 按值记录；undo→redo 往返值不变
/// （旧实现 CreateOp.note.id=0，redo 会重新分配新 id，破坏身份稳定）。
#[test]
fn test_finish_drawing_captures_value_and_redo_preserves_it() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    let drawn = data
        .finish_drawing(0.0, 60, 80.0, 1.0, 80.0)
        .expect("绘制应成功");
    assert_eq!((drawn.tick, drawn.key, drawn.length), (0.0, 60, 80.0));
    let stored = data.current_track_notes().get(0).expect("绘制后音符应存在");
    assert_eq!(stored.start_tick, 0, "落盘值应与绘制值一致");
    assert_eq!(stored.key, 60);

    assert!(data.undo());
    assert_eq!(data.current_track_note_count(), 0, "undo 应删除创建音符");

    assert!(data.redo());
    let restored = data
        .current_track_notes()
        .get(0)
        .expect("redo 后音符应存在");
    assert_eq!(
        (restored.start_tick, restored.key, restored.length()),
        (0, 60, 80),
        "redo 必须原样保留值，不得漂移（身份稳定）"
    );
}

/// 回归：真实绘制路径 undo/redo 的协作广播按值携带（旧实现广播 id=0，
/// 对端无法按值匹配删除/添加）。
#[test]
fn test_create_undo_redo_collab_sync_carries_value() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    data.set_collab_sync_enabled(true);
    let _ = data.finish_drawing(0.0, 60, 80.0, 1.0, 80.0);
    assert_eq!(data.current_track_note_count(), 1);

    assert!(data.undo());
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    // 元组形状 (tick, key, length, velocity, channel, track_index, is_added)
    let (tick, key, len, _vel, _ch, track, is_added) = pending[0];
    assert_eq!(tick, 0.0);
    assert_eq!(key, 60);
    assert_eq!(len, 80.0);
    assert_eq!(track, 1);
    assert!(!is_added, "undo 创建应为删除（is_added=false）");

    assert!(data.redo());
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    let (tick2, key2, len2, _vel2, _ch2, _track2, is_added2) = pending[0];
    assert_eq!(tick2, 0.0);
    assert_eq!(key2, 60);
    assert_eq!(len2, 80.0);
    assert!(is_added2, "redo 创建应为添加（is_added=true）");
}
