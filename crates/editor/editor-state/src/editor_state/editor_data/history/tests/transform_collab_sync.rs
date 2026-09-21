use super::*;
use crate::EditorData;
use crate::editor_transform::EditorTransform;

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
    del_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
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
