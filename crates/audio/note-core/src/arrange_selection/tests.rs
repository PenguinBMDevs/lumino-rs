//! `ArrangeSelection` 单元测试
//!
//! 2026-09 自 `arrange_selection.rs` 拆出，保持主文件 ≤400 行。

use super::ArrangeSelection;
use crate::arrange_selection::FrozenNotes;

#[test]
fn test_contains_and_offset() {
    let mut sel = ArrangeSelection::new();
    sel.add_rect_track(100, 200, 0, 127, 0, 2);
    assert!(sel.contains(1, 150, 60));
    assert!(!sel.contains(3, 150, 60));

    sel.offset_ticks(50);
    assert!(sel.contains(1, 210, 60));
    assert!(sel.contains(1, 150, 60));

    sel.offset_tracks(1);
    assert!(sel.contains(2, 210, 60));
    assert!(!sel.contains(0, 210, 60));
}

#[test]
fn test_freeze_contains_is_exact() {
    // 冻结为两个精确音符：矩形平移覆盖不到的落点音符不再被误伤
    let mut sel = ArrangeSelection::new();
    sel.freeze([(0u16, 300u32, 400u32, 60u8), (0, 500, 600, 64)]);

    assert!(sel.contains(0, 300, 60), "冻结音符应命中");
    assert!(sel.contains(0, 500, 64), "冻结音符应命中");
    // 同一矩形区间内的其他 (tick, key) 不得命中（框选误伤修复核心）
    assert!(!sel.contains(0, 300, 64), "同 tick 不同 key 不应命中");
    assert!(!sel.contains(0, 400, 60), "同 key 不同 tick 不应命中");
    assert!(!sel.contains(1, 300, 60), "不同音轨不应命中");
    assert_eq!(sel.frozen().map(FrozenNotes::len), Some(2));
    assert!(!sel.is_empty());
}

#[test]
fn test_freeze_bbox_covers_notes_only() {
    // 冻结后矩形为紧致边界（显示/命中用），四项并集
    let mut sel = ArrangeSelection::new();
    sel.freeze([(2u16, 100u32, 200u32, 60u8), (2, 400, 500, 72)]);
    let (ts, te, kl, kh, tl, th) = sel.rects[0];
    assert_eq!((ts, te, kl, kh, tl, th), (100, 500, 60, 72, 2, 2));
}

#[test]
fn test_freeze_offset_keeps_exact_set_in_sync() {
    let mut sel = ArrangeSelection::new();
    sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
    sel.offset_ticks(100);
    sel.offset_tracks(1);
    assert!(sel.contains(1, 400, 60), "冻结集应随偏移同步");
    assert!(!sel.contains(0, 400, 60));
    assert!(!sel.contains(1, 300, 60));
}

#[test]
fn test_add_rect_drops_frozen_set() {
    // 新框选（添加矩形）回到矩形判定语义
    let mut sel = ArrangeSelection::new();
    sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
    sel.add_rect_track(100, 200, 0, 127, 0, 0);
    assert!(sel.frozen().is_none());
    assert!(sel.contains(0, 150, 60));
    assert!(!sel.contains(0, 300, 60), "冻结集已被新矩形选择取代");
}

#[test]
fn test_clear_resets_frozen_set() {
    let mut sel = ArrangeSelection::new();
    sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
    sel.clear();
    assert!(sel.is_empty());
    assert!(sel.frozen().is_none());
    assert!(!sel.contains(0, 300, 60));
}

/// 派生缓存（走带选区命中音符）依赖 `revision` 判失效：
/// **每个**变更入口都必须 bump，漏一个就会静默读到脏数据。
#[test]
fn test_revision_bumps_on_every_mutation() {
    let mut sel = ArrangeSelection::new();
    let base = sel.revision();

    sel.add_rect_track(100, 200, 0, 127, 0, 0);
    assert_ne!(sel.revision(), base, "add_rect_track 必须 bump");

    let after_rect = sel.revision();
    sel.offset_ticks(10);
    assert_ne!(sel.revision(), after_rect, "offset_ticks 必须 bump");

    let after_ticks = sel.revision();
    sel.offset_tracks(1);
    assert_ne!(sel.revision(), after_ticks, "offset_tracks 必须 bump");

    let after_tracks = sel.revision();
    sel.offset(10, 1);
    assert_ne!(sel.revision(), after_tracks, "offset 必须 bump");

    let after_offset = sel.revision();
    sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
    assert_ne!(sel.revision(), after_offset, "freeze 必须 bump");

    let after_freeze = sel.revision();
    sel.clear();
    assert_ne!(sel.revision(), after_freeze, "clear 必须 bump");
}

/// 纯几何平移也必须 bump：音符数据没变，但命中集合会变。
/// 这是「选区 revision」与「音符 gen」两个键都必需的原因。
#[test]
fn test_revision_bumps_on_pure_geometry_change() {
    let mut sel = ArrangeSelection::new();
    sel.add_rect_track(100, 200, 0, 127, 0, 0);
    let before = sel.revision();
    // 矩形在界内平移：既不增删矩形也不改冻结集
    sel.offset_ticks(0);
    assert_ne!(
        sel.revision(),
        before,
        "offset_ticks(0) 也是一次变更，必须 bump（否则平移后读到脏缓存）"
    );
}
