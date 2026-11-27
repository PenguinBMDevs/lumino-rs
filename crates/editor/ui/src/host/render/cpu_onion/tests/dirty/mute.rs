//! 静音轨：本帧不画 / 几何当场补齐（`tests_dirty.rs` 的子模块）。

use lumino_midi_model::NoteEvent;

use super::{
    MUTED, alpha, change, dirty, document, key, note_row, reference_pixels_with, stretch_note,
    tracks,
};
use crate::host::render::cpu_onion::CpuOnionState;

/// 静音轨编辑：CPU 不画 → 不重绘、不递增版本（索引照样增量更新）。
#[test]
fn muted_track_edit_skips_redraw() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let muted_note = NoteEvent::new(10, 110, 122, 100, 0);
    document.remove_note(2, 0);
    let output = state.update(
        key(2, 0),
        &document,
        &MUTED,
        Some(dirty(vec![change(2, Some(muted_note), None)])),
        Some(&tracks(&[2])),
    );
    assert!(!output.rebuilt, "静音轨由 CPU 跳过，不得重绘");
    assert_eq!(
        output.frame.expect("应复用").version,
        first.version,
        "不得递增版本"
    );
}

/// **静音非当前轨的直接编辑**（协作远端 / 未来直写路径）：索引几何必须当场补齐。
///
/// 否则取消静音只是布局变化（只重绘、不重建索引），长音符候选仍是旧值 →
/// **静默漏画**。这里把静音轨 2 的音符拉成 2000 tick 长音符，取消静音后必须可见。
#[test]
fn muted_non_current_direct_edit_repairs_index_before_unmute() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &document, &MUTED, None, None);

    // 直接改静音轨 2（不经记录点）：dirty=None，来源点名轨 2
    stretch_note(&mut document, 2, 0, 2010);
    let silent = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[2])));
    assert!(!silent.rebuilt, "静音轨本帧不画，不得重绘");

    // 取消静音 → 布局变化整帧重绘；长音符必须仍在（索引几何已补齐）
    let unmuted = [false, false, false];
    let mut unmuted_key = key(2, 0);
    unmuted_key.mute_fingerprint = 11;
    let output = state.update(unmuted_key, &document, &unmuted, None, Some(&tracks(&[2])));
    assert!(output.rebuilt, "取消静音必须整帧重绘");
    let frame = output.frame.expect("应生成帧");
    assert_eq!(
        frame.pixels,
        reference_pixels_with(&document, 0, &unmuted),
        "取消静音后长音符必须被画出（索引几何已补齐，否则长音符候选过期）"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(122)),
        255,
        "静音轨 2 的长音符在取消静音后必须可见"
    );
}

/// 静音轨直接编辑**不得**触发本帧重绘（CPU 不画它），但索引必须被修正。
///
/// 与上一条互补：这条只断言「当场不重绘」，上一条断言「稍后取消静音可见」。
#[test]
fn muted_non_current_direct_edit_keeps_frame_frozen() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    stretch_note(&mut document, 2, 0, 2010);
    let output = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[2])));
    assert!(!output.rebuilt, "静音轨编辑不得重绘");
    let frame = output.frame.expect("应复用上一帧");
    assert_eq!(frame.version, first.version, "不得递增版本");
    assert!(
        std::sync::Arc::ptr_eq(&first.pixels, &frame.pixels),
        "必须复用同一份像素"
    );
}
