//! async_commit 异步提交单元测试

use super::EditorData;
use crate::DragState;
use bit_vec::BitVec;
use lumino_note_core::history::MoveOp;
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
fn test_async_commit_applies_and_pushes_history() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);
        ds
    });

    assert!(
        data.apply_move_ops_async(ops.clone(), 127)
            .expect("异步移动提交应成功")
    );
    let modified = loop {
        if let Some(result) = data.poll_async_commit() {
            break result.expect("异步提交应成功");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
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
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        25.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        62
    );
    assert!(data.history.can_undo());
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
}

#[test]
fn test_async_commit_preserves_note_length() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(100, 0);
        ds
    });

    assert!(
        data.apply_move_ops_async(ops.clone(), 127)
            .expect("异步移动提交应成功")
    );
    let modified = loop {
        if let Some(result) = data.poll_async_commit() {
            break result.expect("异步提交应成功");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(modified, 1);
    // 右移 100：tick 0 → 100，长度必须保持
    let view = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view.tick, 100.0);
    assert_eq!(view.length, 1.0, "移动后长度必须保持 1.0");
    // undo 恢复原位置，长度同样保持
    assert!(data.undo());
    let view = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view.tick, 0.0);
    assert_eq!(view.length, 1.0, "undo 后长度必须保持 1.0");
    // redo 再次应用，长度依旧保持
    assert!(data.redo());
    let view = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view.tick, 100.0);
    assert_eq!(view.length, 1.0, "redo 后长度必须保持 1.0");
}

#[test]
fn test_async_commit_zero_delta_is_noop() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 2,
        delta_tick: 0,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    assert!(
        !data
            .apply_move_ops_async(ops, 127)
            .expect("异步移动提交应成功")
    );
    assert!(!data.has_pending_commit());
}

#[test]
fn test_async_commit_rejects_concurrent() {
    let mut data = make_data_with_notes();
    let ops1 = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 1,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    let ops2 = vec![MoveOp {
        track_id: 1,
        range_start: 1,
        range_end: 2,
        delta_tick: 1,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    assert!(
        data.apply_move_ops_async(ops1, 127)
            .expect("异步移动提交应成功")
    );
    assert!(data.apply_move_ops_async(ops2, 127).is_err());
}

#[test]
fn test_poll_async_commit_returns_none_while_pending() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 100,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    assert!(
        data.apply_move_ops_async(ops, 127)
            .expect("异步移动提交应成功")
    );
    // 立即轮询可能返回 None（线程尚未完成）
    if data.poll_async_commit().is_none() {
        // 等待完成
        loop {
            if let Some(result) = data.poll_async_commit() {
                result.expect("异步结果应成功");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    assert!(!data.has_pending_commit());
}

#[test]
fn test_cancel_async_commit() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 10,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    assert!(
        data.apply_move_ops_async(ops, 127)
            .expect("异步移动提交应成功")
    );
    data.cancel_async_commit();
    assert!(!data.has_pending_commit());
    // 数据不应被修改
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
}
