//! EditorData 历史记录与 MoveOp 单元测试
//!
//! 覆盖：
//! - `apply_move_ops` 正向/反向应用
//! - key clamp、跨轨操作（document 轨道构造时固定）
//! - `move_ops_from_drag_state` 连续区间拆分、delta 饱和
//! - 基于 MoveOp 的 undo/redo 往返

use super::*;
use crate::DragState;
use crate::editor_transform::EditorTransform;
use bit_vec::BitVec;
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::CreateOp;
use lumino_note_core::note::Note;

fn make_data_with_notes() -> EditorData {
    EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    )
}

#[test]
fn test_apply_move_ops_forward() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 2,
        delta_tick: 5,
        delta_key: -2,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    let modified = data.apply_move_ops(&ops, false, 127);
    assert_eq!(modified, 2);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        5.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        58
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        15.0
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0,
        "未在范围内的音符不变"
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // document 同步更新（唯一权威源）
    let track = data.track_notes(1);
    assert_eq!(track[0].start_tick as f32, 5.0);
    assert_eq!(track[1].start_tick as f32, 15.0);
}

#[test]
fn test_apply_move_ops_inverse() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 3,
        delta_tick: 10,
        delta_key: 5,
        seq: 0,
        original_ticks: vec![0.0, 10.0, 20.0],
        original_keys: vec![60, 62, 64],
    }];
    // 先 forward
    data.apply_move_ops(&ops, false, 127);
    // 再 inverse 应还原
    let modified = data.apply_move_ops(&ops, true, 127);
    assert_eq!(modified, 3);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        10.0
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        62
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );
}

#[test]
fn test_apply_move_ops_clamps_key() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 0,
        delta_key: -100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 20);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        0,
        "key 应 clamp 到 0"
    );

    let ops2 = vec![MoveOp {
        track_id: 1,
        range_start: 1,
        range_end: 2,
        delta_tick: 0,
        delta_key: 100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops2, false, 20);
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        20,
        "key 应 clamp 到 max_key"
    );
}

#[test]
fn test_apply_move_ops_creates_missing_track_notes() {
    // 语义替代（2026-08）：apply_move_ops 不再自动创建缺失音轨（document 轨道
    // 构造时固定）。原测试意图「操作指定轨数据」改为：构造含 track 2 的 document，
    // 验证 apply_move_ops 可作用于非当前轨（track_id=2）。
    let mut data = EditorData::with_f32_notes(2, &[Note::new(0.0, 60, 1.0)]);
    let ops = vec![MoveOp {
        track_id: 2,
        range_start: 0,
        range_end: 1,
        delta_tick: 3,
        delta_key: 1,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 127);
    let track = data.track_notes(2);
    assert_eq!(track[0].start_tick as f32, 3.0);
    assert_eq!(track[0].key as u16, 61);
}

#[test]
fn test_move_ops_from_drag_state_splits_ranges() {
    let data = EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
            Note::new(30.0, 66, 1.0),
        ],
    );

    let mut bv = BitVec::from_elem(4, false);
    bv.set(0, true);
    bv.set(1, true);
    bv.set(3, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.set_delta(5, -2);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops.len(), 2, "应拆分为两个连续段");
    assert_eq!(ops[0].range_start, 0);
    assert_eq!(ops[0].range_end, 2);
    assert_eq!(ops[0].delta_tick, 5);
    assert_eq!(ops[0].delta_key, -2);
    assert_eq!(ops[0].seq, 0);

    assert_eq!(ops[1].range_start, 3);
    assert_eq!(ops[1].range_end, 4);
    assert_eq!(ops[1].delta_tick, 5);
    assert_eq!(ops[1].delta_key, -2);
    assert_eq!(ops[1].seq, 1);
}

#[test]
fn test_move_ops_from_drag_state_saturates_delta_tick() {
    let data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);

    let mut drag_state = DragState::from_single(0, data.current_track_note_count(), 0, 60);
    drag_state.set_delta(i64::MAX, 0);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MAX, "delta_tick 应饱和到 i32::MAX");

    drag_state.set_delta(i64::MIN, 0);
    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MIN, "delta_tick 应饱和到 i32::MIN");
}

#[test]
fn test_undo_redo_with_move_op_entry() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);
        ds
    });
    data.apply_move_ops(&ops, false, 127);
    data.push_move_op(ops);

    // undo 应还原
    assert!(data.undo());
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // redo 应再次应用
    assert!(data.redo());
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        5.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        58
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        25.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        62
    );
}

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
    pending.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
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
    pending2.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
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
    data.apply_create_ops(&[op.clone()], false);
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

/// Bug 回归：变换类操作（变速/移调/翻转/批量编辑）前向与 undo/redo 均须广播同步。
/// 此前这些操作只改 document + 推整轨快照历史，既不在前向发射同步事件，也不在
/// undo/redo 回放时广播，导致 B 端永久失同步（用户报告「A 用变速工具后 B 不同步」）。
///
/// 本测试覆盖：
/// 1. 前向 `apply_speed_change` 入队 `(Delete 旧, Add 新)`，且旧/新状态正确；
/// 2. undo（整轨快照回放）入队「全删旧 + 全加新」对账，使 B 终态与 A 一致。
#[test]
fn test_speed_change_populates_collab_transform_sync() {
    use std::collections::HashSet;
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0)]);

    // ── 前向：速度系数 2.0（min_tick=0）→ 长度翻倍，tick 不变 ──
    let selected = HashSet::from([0usize]);
    let modified = data.apply_speed_change(&selected, 2.0);
    assert_eq!(modified, 1, "变速应修改 1 个音符");
    assert_eq!(data.current_track_note_count(), 1);
    let pending = data.take_pending_collab_transform_sync();
    // 每个变化音符 → 一条 Delete(旧) + 一条 Add(新)；元组 (is_add, id, t, k, l, v, c, tr)
    assert_eq!(pending.len(), 2);
    let (is_add_0, _id0, t0, k0, l0, _v0, _c0, _tr0) = pending[0];
    let (is_add_1, _id1, t1, k1, l1, _v1, _c1, _tr1) = pending[1];
    assert!(!is_add_0, "前向第一条应为删除旧音符");
    assert_eq!((t0, k0, l0), (0.0, 60, 1.0));
    assert!(is_add_1, "前向第二条应为添加新音符");
    assert_eq!((t1, k1, l1), (0.0, 60, 2.0));

    // ── undo：整轨快照回放，入队「全删当前 + 全加快照(旧)」对账 ──
    assert!(data.undo());
    let pending2 = data.take_pending_collab_transform_sync();
    // 单音符：删除当前(长度2.0) + 添加快照(长度1.0)
    assert_eq!(pending2.len(), 2);
    let (ia, _ida, ta, ka, la, _, _, _) = pending2[0];
    let (ib, _idb, tb, kb, lb, _, _, _) = pending2[1];
    assert!(!ia, "undo 第一条应为删除当前(新)音符");
    assert_eq!((ta, ka, la), (0.0, 60, 2.0));
    assert!(ib, "undo 第二条应为添加快照(旧)音符");
    assert_eq!((tb, kb, lb), (0.0, 60, 1.0));
    assert_eq!(data.current_track_note_count(), 1);
}

/// Bug 回归：移调（transpose）前向须广播旧→新（key 变化）的删除+添加，
/// 使 B 端在 A 移调后同步音高。
#[test]
fn test_transpose_populates_collab_transform_sync() {
    use std::collections::HashSet;
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0)]);
    let selected = HashSet::from([0usize]);
    let modified = data.transpose(&selected, 3);
    assert_eq!(modified, 1, "移调应修改 1 个音符");
    let pending = data.take_pending_collab_transform_sync();
    assert_eq!(pending.len(), 2);
    let (is_add_0, _id0, t0, k0, _l0, _v0, _c0, _tr0) = pending[0];
    let (is_add_1, _id1, _t1, k1, _l1, _v1, _c1, _tr1) = pending[1];
    assert!(!is_add_0);
    assert_eq!(t0, 0.0);
    assert_eq!(k0, 60, "删除的旧音符 key=60");
    assert!(is_add_1);
    assert_eq!(k1, 63, "添加的新音符 key=63 (+3 半音)");
}

/// Bug 回归：分割（split）改变音符数量，前向须入队「删原 + 加左 + 加右」，
/// 否则 B 端只见新增的左/右之一或完全缺失（用户报告「拆分没同步」）。
#[test]
fn test_split_populates_collab_transform_sync() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0)]);
    let ok = data.split_note(0, 0.5);
    assert!(ok, "split 应成功");
    assert_eq!(data.current_track_note_count(), 2);
    let pending = data.take_pending_collab_transform_sync();
    // 删原(0,60,1.0) + 加左(0,60,0.5) + 加右(0.5,60,0.5)
    assert_eq!(pending.len(), 3);
    let (d, _id0, t0, k0, l0, _, _, _) = pending[0];
    let (a1, _id1, t1, k1, l1, _, _, _) = pending[1];
    let (a2, _id2, t2, k2, l2, _, _, _) = pending[2];
    assert!(!d, "首条应为删除原音符");
    assert_eq!((t0, k0, l0), (0.0, 60, 1.0));
    assert!(a1 && a2, "后两条应为添加左右");
    assert_eq!((t1, k1, l1), (0.0, 60, 0.5));
    assert_eq!((t2, k2, l2), (0.5, 60, 0.5));
}

/// Bug 回归：合并（glue）改变音符数量，前向须入队「删每个被并音符 + 加合并后音符」。
#[test]
fn test_glue_populates_collab_transform_sync() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0), Note::new(1.0, 60, 1.0)]);
    use std::collections::HashSet;
    let merged = data.glue_selected_notes(&HashSet::from([0usize, 1usize]));
    assert_eq!(merged, 1, "应合并 1 组");
    assert_eq!(data.current_track_note_count(), 1);
    let pending = data.take_pending_collab_transform_sync();
    // 删两个被并音符 + 加一个合并后音符(0..2)
    assert_eq!(pending.len(), 3);
    let adds: Vec<_> = pending.iter().filter(|e| e.0).collect();
    let dels: Vec<_> = pending.iter().filter(|e| !e.0).collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(dels.len(), 2);
    let (_, _aid, at, ak, al, _, _, _) = *adds[0];
    assert_eq!((at, ak, al), (0.0, 60, 2.0), "合并后音符应为 (0..2)");
    let mut del_ticks: Vec<f32> = dels.iter().map(|e| e.2).collect();
    del_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(del_ticks, vec![0.0, 1.0], "应删除两个被并音符(0 与 1)");
}

/// Bug 回归：连奏（tie）延长长度，前向须入队「删旧长度 + 加新长度」（同 tick/key）。
#[test]
fn test_tie_populates_collab_transform_sync() {
    let mut data =
        EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0), Note::new(2.0, 60, 1.0)]);
    use std::collections::HashSet;
    let tied = data.tie_selected_notes(&HashSet::from([0usize, 1usize]));
    assert_eq!(tied, 1, "应连接 1 个音符");
    let pending = data.take_pending_collab_transform_sync();
    assert_eq!(pending.len(), 2);
    let (d, _id, dt, dk, dl, _, _, _) = pending[0];
    let (a, _id2, at, ak, al, _, _, _) = pending[1];
    assert!(!d && a, "应为删旧长度 + 加新长度");
    assert_eq!((dt, dk, dl), (0.0, 60, 1.0), "旧长度 1.0");
    assert_eq!((at, ak, al), (0.0, 60, 2.0), "新长度延长到 2.0");
}

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
