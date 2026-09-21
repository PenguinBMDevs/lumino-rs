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
