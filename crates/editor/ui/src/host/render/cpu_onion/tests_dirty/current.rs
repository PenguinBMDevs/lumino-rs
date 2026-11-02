//! 当前轨直接编辑（必须不重绘）与切轨补齐（`tests_dirty.rs` 的子模块）。

use std::sync::Arc;

use super::{MUTED, alpha, document, key, note_row, reference_pixels, stretch_note, tracks};
use crate::host::render::cpu_onion::CpuOnionState;

/// **当前轨直接编辑**：不重绘、不递增版本、像素 Arc 复用；冻结键推进到新代次。
#[test]
fn current_track_edit_skips_redraw_and_version() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    let first = state.update(key(1, 0), &document, &MUTED, None, None);
    assert!(first.rebuilt, "首帧必须重新光栅化");
    let first = first.frame.expect("首帧应生成");

    // 直接改当前轨（拖动 / 批量 / 连奏语义）：**Host 实际传入 dirty=None**
    // ——这些路径不经 insert/remove/update，脏区队列自然是空的；
    // `onion_dirty_tracks = Some(empty)` 才是「来源 = 当前轨」的真源。
    stretch_note(&mut document, 0, 0, 900);
    let second = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[])));
    assert!(!second.rebuilt, "当前轨编辑不得触发 CPU 重绘");
    let second = second.frame.expect("应复用上一帧");
    assert_eq!(second.version, first.version, "不得递增版本");
    assert!(
        Arc::ptr_eq(&first.pixels, &second.pixels),
        "必须复用同一份像素（零拷贝零重绘）"
    );

    // 同代次再次调用：冻结键已推进，直接复用
    let third = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[])));
    assert!(!third.rebuilt);
    assert_eq!(
        third.frame.expect("应复用").version,
        first.version,
        "重复帧不得递增版本"
    );
}

/// 同形态但来源点名当前轨（`Some({current})`，连奏 / 批量编辑路径）→ 同样跳过重绘。
#[test]
fn current_track_source_only_skips_redraw() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    stretch_note(&mut document, 0, 0, 900);
    let output = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[0])));
    assert!(!output.rebuilt, "来源=当前轨时不得重绘 CPU 帧");
    let frame = output.frame.expect("应复用上一帧");
    assert_eq!(frame.version, first.version, "不得递增版本");
    assert!(
        Arc::ptr_eq(&first.pixels, &frame.pixels),
        "必须复用同一份像素"
    );
}

/// 同样的跳过语义在「脏区对象存在但原子变更列表为空」时也必须成立。
#[test]
fn current_track_edit_with_empty_dirty_skips_redraw() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    stretch_note(&mut document, 0, 0, 900);
    let second = state.update(
        key(2, 0),
        &document,
        &MUTED,
        Some(super::dirty(Vec::new())),
        Some(&tracks(&[])),
    );
    assert!(!second.rebuilt, "空原子变更列表不得触发重绘");
    assert_eq!(
        second.frame.expect("应复用").version,
        first.version,
        "不得递增版本"
    );
}

/// **当前轨直接编辑后的切轨补齐**：旧当前轨变成洋葱轨时，索引几何必须被修复。
///
/// 用例把当前轨 0 的音符改成 2000 tick 长音符（> LOOKBACK_TICKS）：长音符只从
/// `TrackIndex::long_notes` 取，若切轨时没有 `rebuild_track_geometry`，该音符会
/// **完全消失**（窗口通道按 `length() <= LOOKBACK_TICKS` 过滤掉它）。
#[test]
fn current_track_switch_repairs_stale_index() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &document, &MUTED, None, None);

    // 直接编辑当前轨（拖动/批量语义）：Host 实际传入 dirty=None + Some(empty)
    stretch_note(&mut document, 0, 0, 2010);
    let silent = state.update(key(2, 0), &document, &MUTED, None, Some(&tracks(&[])));
    assert!(!silent.rebuilt, "当前轨编辑不得重绘");

    // 切到轨 1 → 轨 0 变成洋葱轨，必须整帧重绘并补齐它的几何
    let switched = state.update(key(2, 1), &document, &MUTED, None, Some(&tracks(&[])));
    assert!(switched.rebuilt, "切轨必须整帧重绘");
    let frame = switched.frame.expect("应生成帧");
    assert_eq!(
        frame.pixels,
        reference_pixels(&document, 1),
        "切轨后旧当前轨的长音符必须被画出"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(120)),
        255,
        "旧当前轨（轨 0）长音符在切轨后必须可见"
    );
}

/// **同一帧内「直接编辑 + 切轨」**：不经过中间帧，旧当前轨几何也必须被补齐。
///
/// Host 实际形态：拖动那一笔的 `dirty` 已被同一帧的 take 取走（`None`），
/// 只剩 `onion_dirty_tracks = Some(empty)`。
#[test]
fn drag_and_track_switch_in_same_frame_repairs_old_current() {
    let mut document = document();
    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &document, &MUTED, None, None);

    stretch_note(&mut document, 0, 0, 2010);
    let output = state.update(key(2, 1), &document, &MUTED, None, Some(&tracks(&[])));
    assert!(output.rebuilt, "切轨必须整帧重绘");
    let frame = output.frame.expect("应生成帧");
    assert_eq!(
        frame.pixels,
        reference_pixels(&document, 1),
        "同帧切轨时旧当前轨的长音符同样必须被画出"
    );
    assert_eq!(alpha(&frame, 60, note_row(120)), 255);
}
