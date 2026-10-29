//! 洋葱轨脏区重绘与整帧兜底（`tests_dirty.rs` 的子模块）。

use lumino_editor_state::CpuOnionDirty;
use lumino_midi_model::NoteEvent;

use super::{MUTED, alpha, change, dirty, document, key, note_row, reference_pixels, tracks};
use crate::host::render::cpu_onion::CpuOnionState;

/// 洋葱轨单笔编辑：只重画脏区，但整帧像素与整帧重绘逐像素一致。
#[test]
fn onion_single_edit_matches_full_render() {
    let before = document();
    let mut after = before.clone();
    // 轨 1 的音符从 key 121 移到 key 122（更新 = 旧位置清除 + 新位置绘制）
    let old = NoteEvent::new(10, 110, 121, 100, 0);
    let new = NoteEvent::new(10, 110, 122, 100, 0);
    after.remove_note(1, 0);
    after.insert_note(1, new);

    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &before, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let output = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(dirty(vec![change(1, Some(old), Some(new))])),
        Some(&tracks(&[1])),
    );
    assert!(output.rebuilt, "洋葱轨变化必须重绘");
    let patched = output.frame.expect("应生成帧");
    assert_ne!(patched.version, first.version, "重绘必须递增版本");
    assert_eq!(
        patched.pixels,
        reference_pixels(&after, 0),
        "区域重绘必须与整帧重绘逐像素一致"
    );

    // 旧位置（key 121）必须被清空，新位置（key 122）必须绘制
    assert_eq!(alpha(&patched, 60, note_row(121)), 0, "旧位置必须清空");
    assert_eq!(alpha(&patched, 60, note_row(122)), 255, "新位置必须绘制");
}

/// 批量/整轨替换（`dirty.full`）：整帧重绘，像素与整帧渲染一致。
#[test]
fn dirty_full_redraws_whole_frame() {
    let before = document();
    let mut after = before.clone();
    after.insert_note(1, NoteEvent::new(300, 400, 118, 100, 0));
    after.insert_note(1, NoteEvent::new(500, 600, 117, 100, 0));

    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &before, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let output = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(CpuOnionDirty {
            full: true,
            changes: Vec::new(),
        }),
        Some(&tracks(&[1])),
    );
    assert!(output.rebuilt, "整帧兜底必须重绘");
    let frame = output.frame.expect("应生成帧");
    assert_ne!(frame.version, first.version);
    assert_eq!(frame.pixels, reference_pixels(&after, 0));
}

/// 未被 changes 覆盖的洋葱轨变化：保守整帧。
#[test]
fn uncovered_onion_dirty_track_falls_back_to_full() {
    let before = document();
    let mut after = before.clone();
    after.insert_note(1, NoteEvent::new(300, 400, 118, 100, 0));

    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &before, &MUTED, None, None);

    let output = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(dirty(Vec::new())),
        // 轨 1 是洋葱轨却没有对应 change → 必须整帧兜底
        Some(&tracks(&[1])),
    );
    assert!(output.rebuilt, "未覆盖的洋葱轨变化必须整帧");
    assert_eq!(
        output.frame.expect("应生成帧").pixels,
        reference_pixels(&after, 0)
    );
}

/// `dirty=None` 但来源点名**洋葱轨**（直接编辑且未记录）→ 整帧 + 版本递增。
///
/// 这是「不能把没有 changes 的洋葱轨编辑静默放过」的守卫：CPU 帧画的正是它。
#[test]
fn dirty_none_onion_source_falls_back_to_full() {
    let before = document();
    let mut after = before.clone();
    after.insert_note(1, NoteEvent::new(300, 400, 118, 100, 0));

    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &before, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let output = state.update(key(2, 0), &after, &MUTED, None, Some(&tracks(&[1])));
    assert!(output.rebuilt, "未记录的洋葱轨编辑必须整帧兜底");
    let frame = output.frame.expect("应生成帧");
    assert_ne!(frame.version, first.version, "整帧兜底必须递增版本");
    assert_eq!(frame.pixels, reference_pixels(&after, 0));
    assert_eq!(
        alpha(&frame, 350, note_row(118)),
        255,
        "洋葱轨新增音符必须被画出"
    );
}
