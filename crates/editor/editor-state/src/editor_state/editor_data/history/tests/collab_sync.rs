use super::*;
use crate::DragState;
use crate::EditorData;
use bit_vec::BitVec;
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::CreateOp;

/// Bug 2 回归：A 端对 MoveOp 执行 undo/redo 后，必须把「反向/正向移动」记入
/// `pending_collab_move_sync`，供 ui-editor 层广播给协作对端。若缺失，B 端在 A 撤销后
/// 本地音符坐标与 A 端失同步（用户日志中的「匹配 0/1」「B 无法正确响应」）。
#[test]
fn test_undo_redo_populates_collab_move_sync() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2); // delta_tick=5, delta_key=-2
        ds
    });
    data.apply_move_ops(&ops, false, 127);
    data.push_move_op(ops);

    // 移动/提交阶段不应累积协作同步记录
    assert!(
        data.take_pending_collab_move_sync().is_empty(),
        "提交阶段不应填充待广播队列"
    );

    // ── undo（inverse=true）──
    assert!(data.undo());
    let mut pending = data.take_pending_collab_move_sync();
    assert_eq!(pending.len(), 2, "被移动的两个音符应各有一条同步记录");
    // 元组形状 (id, tick, key, tick_offset, key_offset, track_index)
    pending.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let (id0, t0, k0, to0, ko0, tr0) = pending[0];
    assert!(id0 > 0, "音符应已分配全局唯一 id");
    // 音符0：original_tick=0, original_key=60, delta_tick=5, delta_key=-2
    //  ref = original + delta，offset = -delta
    assert_eq!((t0, k0, to0, ko0, tr0), (5.0, 58, -5.0, 2, 1));
    let (id1, t1, k1, to1, ko1, tr1) = pending[1];
    assert!(id1 > 0, "音符应已分配全局唯一 id");
    // 音符2：original_tick=20, original_key=64
    assert_eq!((t1, k1, to1, ko1, tr1), (25.0, 62, -5.0, 2, 1));

    // ── redo（inverse=false）──
    assert!(data.redo());
    let mut pending2 = data.take_pending_collab_move_sync();
    assert_eq!(pending2.len(), 2);
    pending2.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    // ref = original，offset = +delta
    let (id2, t2, k2, to2, ko2, tr2) = pending2[0];
    assert!(id2 > 0, "音符应已分配全局唯一 id");
    assert_eq!((t2, k2, to2, ko2, tr2), (0.0, 60, 5.0, -2, 1));
    let (id3, t3, k3, to3, ko3, tr3) = pending2[1];
    assert!(id3 > 0, "音符应已分配全局唯一 id");
    assert_eq!((t3, k3, to3, ko3, tr3), (20.0, 64, 5.0, -2, 1));
}

/// Bug 回归：A 端撤销「创建音符」时本地音符消失，但此前不广播删除事件，
/// 导致 B 端残留该音符。本测试验证 undo/redo 创建会正确填充
/// `pending_collab_create_sync`（undo→`LocalNoteDeleted`，redo→`LocalNoteAdded`）。
#[test]
fn test_undo_redo_create_populates_collab_create_sync() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0)]);
    // 模拟「创建」一个位于 (100, 72) 的新音符（携带真实全局 id，等同 finish_drawing 分配）
    let op = CreateOp {
        track_id: 0,
        note: NoteEvent::new(100, 101, 72, 100, 0).with_id(7),
    };
    // 正向应用（创建）
    data.apply_create_ops(std::slice::from_ref(&op), false);
    assert_eq!(data.current_track_note_count(), 2, "创建后应有 2 个音符");
    data.push_note_create(vec![op]);

    assert!(
        data.take_pending_collab_create_sync().is_empty(),
        "创建提交阶段不应填充队列"
    );

    // ── undo：本地删除，应广播 LocalNoteDeleted（is_added=false）──
    assert!(data.undo());
    assert_eq!(
        data.current_track_note_count(),
        1,
        "撤销创建后本地只剩原音符"
    );
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    // 元组形状 (id, tick, key, length, velocity, channel, track_index, is_added)
    let (id, tick, key, _len, _vel, _ch, track, is_added) = pending[0];
    assert!(id > 0, "新建音符应已分配全局唯一 id");
    assert_eq!(tick, 100.0);
    assert_eq!(key, 72);
    assert_eq!(track, 0);
    assert!(!is_added, "undo 创建应为删除（is_added=false）");

    // ── redo：本地重新插入，应广播 LocalNoteAdded（is_added=true）──
    assert!(data.redo());
    assert_eq!(data.current_track_note_count(), 2);
    let pending2 = data.take_pending_collab_create_sync();
    assert_eq!(pending2.len(), 1);
    let (id2, tick2, key2, _len2, _vel2, _ch2, _track2, is_added2) = pending2[0];
    assert!(id2 > 0, "新建音符应已分配全局唯一 id");
    assert_eq!(tick2, 100.0);
    assert_eq!(key2, 72);
    assert!(is_added2, "redo 创建应为添加（is_added=true）");
}
