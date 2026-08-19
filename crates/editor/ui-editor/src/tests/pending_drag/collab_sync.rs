//! 协作同步回归测试：批量移动（框选拖动）必须广播 `LocalNoteMoved`。
//!
//! 回归背景：此前 `commit_pending_drag` 提交框选移动时完全不广播协作事件，
//! 导致协作对端的音符状态不会改变（「A 端移动，B 端不动」）。本测试验证
//! 批量移动提交后会为**每个被选中音符**发射一次 `LocalNoteMoved` 同步事件，
//! 携带原始位置与统一偏移，供 Runner 转换为 `NoteBatchOperation(Move)` 广播。
//!
//! 注意：事件缓冲区是全局单例，本 crate 内其它并行测试也会发射
//! `LocalNoteMoved`（修复后所有批量移动路径都会广播）。因此本测试使用
//! **唯一魔数**（tick/key/偏移组合在仓库内其它测试中不会出现），按精确签名
//! 过滤，从而与并行污染完全隔离，避免误判。

use super::commit_pending_drag_and_drain;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;
use lumino_message::events::window::sync::Event as SyncEvent;
use lumino_message::events::{self, Event};
use std::sync::Mutex;

// 全局事件缓冲区是单例，跨测试并行运行会污染，串行化本模块事件断言。
// 使用 `unwrap_or_else` 从中毒中恢复，避免某测试 panic 后级联炸毁其它测试。
static EVENT_TEST_MUTEX: Mutex<()> = Mutex::new(());

/// 唯一魔数：本仓库其它测试不会用到该偏移组合，用于从并行污染中精确隔离。
const SIG_TICK_OFFSET: f32 = 321.0;
const SIG_KEY_OFFSET: i16 = 11;

fn lock_event_guard() -> std::sync::MutexGuard<'static, ()> {
    EVENT_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
}

/// 取出当前缓冲区中匹配本测试唯一签名的 `LocalNoteMoved` 事件。
fn take_signature_moves() -> Vec<(f32, u16, f32, i16, usize)> {
    events::take_events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Window(lumino_message::events::window::Event::Sync(
                SyncEvent::LocalNoteMoved {
                    tick,
                    key,
                    tick_offset,
                    key_offset,
                    track_index,
                    ..
                },
            )) if tick_offset == SIG_TICK_OFFSET
                && key_offset == SIG_KEY_OFFSET
                && track_index == 0 =>
            {
                Some((tick, key, tick_offset, key_offset, track_index))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn test_commit_pending_drag_broadcasts_each_selected_note() {
    let _guard = lock_event_guard();
    // 清空可能残留的事件，避免其它并行测试污染断言
    let _ = events::take_events();

    let mut editor = Editor::new();
    // 注意：seed_notes 会按 tick 升序重排音符，重排后索引如下：
    //   idx0 = (999, 50)  ← 选中
    //   idx1 = (1234, 71) ← 未选中
    //   idx2 = (2345, 73) ← 选中
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(1234.0, 71, 480.0),
            Note::new(999.0, 50, 240.0),
            Note::new(2345.0, 73, 240.0),
        ],
    );

    // 框选索引 0 与 2，统一偏移 (tick=321, key=11) —— 唯一魔数
    let mut drag = DragState::from_indices([0, 2], 3, 0, 71);
    drag.set_delta(SIG_TICK_OFFSET as i64, SIG_KEY_OFFSET);
    editor.pending_drag_state = Some(drag);

    assert!(
        commit_pending_drag_and_drain(&mut editor),
        "批量移动应成功启动异步提交"
    );

    let moved = take_signature_moves();

    // 应恰好为 2 个被选中音符各广播一次
    assert_eq!(
        moved.len(),
        2,
        "批量移动应为每个被选中音符广播一次 LocalNoteMoved，实际: {:?}",
        moved
    );

    // 原始位置 (999.0, 50) 的音符应被广播（选中 idx0）
    assert!(
        moved
            .iter()
            .any(|(t, k, ..)| (*t - 999.0).abs() < 1.0 && *k == 50),
        "应包含原始位置 (999, 50) 的音符，实际: {:?}",
        moved
    );
    // 原始位置 (2345.0, 73) 的音符应被广播（选中 idx2）
    assert!(
        moved
            .iter()
            .any(|(t, k, ..)| (*t - 2345.0).abs() < 1.0 && *k == 73),
        "应包含原始位置 (2345, 73) 的音符，实际: {:?}",
        moved
    );
    // 未选中的 (1234.0, 71) 不应被广播（idx1 未选中）
    assert!(
        !moved
            .iter()
            .any(|(t, k, ..)| (*t - 1234.0).abs() < 1.0 && *k == 71),
        "未选中的音符不应被广播，实际: {:?}",
        moved
    );
    // 所有广播应携带相同的统一偏移与音轨
    for (.., tick_offset, key_offset, track_index) in &moved {
        assert_eq!(*tick_offset, SIG_TICK_OFFSET, "tick 偏移应一致");
        assert_eq!(*key_offset, SIG_KEY_OFFSET, "key 偏移应一致");
        assert_eq!(*track_index, 0, "音轨索引应一致");
    }
}

#[test]
fn test_commit_pending_drag_no_broadcast_when_empty_selection() {
    let _guard = lock_event_guard();
    let _ = events::take_events();

    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(1234.0, 71, 480.0)]);
    // 空选中（无任何索引），使用唯一魔数偏移，不应广播任何事件。
    // 空选区无操作可提交，`commit_pending_drag` 返回 false（属正常行为）。
    let mut drag = DragState::from_indices([], 1, 0, 71);
    drag.set_delta(SIG_TICK_OFFSET as i64, SIG_KEY_OFFSET);
    editor.pending_drag_state = Some(drag);

    assert!(
        !commit_pending_drag_and_drain(&mut editor),
        "空选区无操作可提交，应返回 false"
    );

    // 以唯一签名过滤：空选区不应产生任何匹配事件
    assert_eq!(
        take_signature_moves().len(),
        0,
        "无选中音符时不应广播任何 LocalNoteMoved"
    );
}
