//! InteractionState 单元测试

use super::*;

#[test]
fn test_edit_state_default_is_idle() {
    assert_eq!(EditState::default(), EditState::Idle);
}

#[test]
fn test_hit_type_variants() {
    assert_ne!(HitType::Start, HitType::Middle);
    assert_ne!(HitType::Middle, HitType::End);
}

#[test]
fn test_selection_hit_type_variants() {
    let variants = [
        SelectionHitType::Inside,
        SelectionHitType::LeftEdge,
        SelectionHitType::RightEdge,
    ];
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

#[test]
fn test_dragging_selection_copy_variant() {
    use crate::editor_state::drag_state::DragState;
    // 复制拖动变体携带独立 DragState，可区分于移动拖动
    let copy_state = EditState::DraggingSelectionCopy {
        drag_state: DragState::default(),
    };
    let move_state = EditState::DraggingSelection {
        drag_state: DragState::default(),
    };
    assert_ne!(copy_state, move_state);
    assert_ne!(copy_state, EditState::default());
}

#[test]
fn test_take_audio_actions() {
    let mut state = InteractionState::default();
    state.push_audio_action(AudioAction::PlayNote {
        key: 60,
        velocity: 100,
    });
    let actions = state.take_audio_actions();
    assert_eq!(actions.len(), 1);
    assert!(state.pending_audio_actions.is_empty());
}

#[test]
fn test_play_note_audio() {
    let mut state = InteractionState::default();
    state.play_note_audio(72, 80);
    assert_eq!(state.pending_audio_actions.len(), 1);
}

#[test]
fn test_set_preview_sequence_replaces_old() {
    let mut state = InteractionState::default();
    let now = Instant::now();
    let note = |play_at: Instant, key: u8| PreviewSequenceNote {
        play_at,
        key,
        velocity: 100,
    };
    state.set_preview_sequence(vec![note(now, 60), note(now, 62)]);
    // 替换旧序列：旧序列被清空
    state.set_preview_sequence(vec![note(now, 64)]);
    assert_eq!(state.preview_sequence.len(), 1);
    assert_eq!(state.preview_sequence[0].key, 64);
}

#[test]
fn test_drain_preview_sequence_by_play_time() {
    let mut state = InteractionState::default();
    let t0 = Instant::now();
    // 按 BPM 时序：0ms、500ms、1000ms 各一个音符
    state.set_preview_sequence(vec![
        PreviewSequenceNote {
            play_at: t0,
            key: 60,
            velocity: 100,
        },
        PreviewSequenceNote {
            play_at: t0 + std::time::Duration::from_millis(500),
            key: 62,
            velocity: 100,
        },
        PreviewSequenceNote {
            play_at: t0 + std::time::Duration::from_millis(1000),
            key: 64,
            velocity: 100,
        },
    ]);

    // t=0：第一个音符立即弹出
    assert_eq!(drain_at(&mut state, t0), Some(60));
    // t=100ms：第二个还没到
    assert_eq!(
        drain_at(&mut state, t0 + std::time::Duration::from_millis(100)),
        None,
        "未到 play_at 的音符不应弹出"
    );
    // t=500ms：第二个到达
    assert_eq!(
        drain_at(&mut state, t0 + std::time::Duration::from_millis(500)),
        Some(62)
    );
    // t=900ms：第三个还没到
    assert_eq!(
        drain_at(&mut state, t0 + std::time::Duration::from_millis(900)),
        None
    );
    // t=1000ms：第三个到达，且同帧到期的可合并弹出
    assert_eq!(
        drain_at(&mut state, t0 + std::time::Duration::from_millis(1000)),
        Some(64)
    );
    assert!(state.preview_sequence.is_empty(), "序列应播放完毕");
}

#[test]
fn test_drain_preview_sequence_merges_due_notes() {
    let mut state = InteractionState::default();
    let t0 = Instant::now();
    // 两个音符同一时刻到期：一次 drain 应全部弹出（保持正确时序，不丢帧）
    state.set_preview_sequence(vec![
        PreviewSequenceNote {
            play_at: t0,
            key: 60,
            velocity: 100,
        },
        PreviewSequenceNote {
            play_at: t0 + std::time::Duration::from_millis(100),
            key: 62,
            velocity: 100,
        },
    ]);
    let now = t0 + std::time::Duration::from_millis(500);
    state.drain_preview_sequence(now);
    let keys: Vec<u8> = state
        .pending_audio_actions
        .iter()
        .filter_map(|a| match a {
            AudioAction::PlayNote { key, .. } => Some(*key),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec![60, 62], "到期的音符应按序全部弹出");
    assert!(state.preview_sequence.is_empty());
}

#[test]
fn test_drain_preview_sequence_after_clear_noop() {
    let mut state = InteractionState::default();
    let now = Instant::now();
    state.set_preview_sequence(vec![PreviewSequenceNote {
        play_at: now,
        key: 60,
        velocity: 100,
    }]);
    state.clear_preview_sequence();
    assert!(state.preview_sequence.is_empty());
    assert_eq!(drain_at(&mut state, now), None);
    assert!(state.pending_audio_actions.is_empty());
}

#[test]
fn test_clear_preview_sequence_discards_pending() {
    let mut state = InteractionState::default();
    let now = Instant::now();
    state.set_preview_sequence(vec![PreviewSequenceNote {
        play_at: now,
        key: 60,
        velocity: 100,
    }]);
    // 弹出第一个后清空，再设置新序列：新序列按自身 play_at 播放
    let _ = drain_at(&mut state, now);
    state.clear_preview_sequence();
    state.set_preview_sequence(vec![PreviewSequenceNote {
        play_at: now + std::time::Duration::from_millis(200),
        key: 70,
        velocity: 100,
    }]);
    assert_eq!(drain_at(&mut state, now), None, "未到 play_at 不应弹出");
    assert_eq!(
        drain_at(&mut state, now + std::time::Duration::from_millis(200)),
        Some(70)
    );
}

/// 单次弹出辅助：在 `now` 时刻调用 `drain_preview_sequence`，
/// 取出并清空音频动作，返回本次弹出的第一个音符 key（无弹出则 None）。
fn drain_at(state: &mut InteractionState, now: Instant) -> Option<u8> {
    state.drain_preview_sequence(now);
    let action = state.pending_audio_actions.drain(..).next();
    match action {
        Some(AudioAction::PlayNote { key, .. }) => Some(key),
        _ => None,
    }
}
