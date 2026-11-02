//! 洋葱轨脏区与整帧兜底（`plan/tests.rs` 的子模块）。

use lumino_midi_model::NoteEvent;

use super::super::{CpuOnionIndex, CpuOnionRedraw, DIRTY_PAD_PX, plan_update};
use super::{assert_full_rebuild, base, camera, change, dirty_with, tracks};

/// 洋葱轨单音编辑 → 区域重绘（矩形覆盖 old/new 并含 4px 余量）。
#[test]
fn onion_single_edit_redraws_region_only() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let old = NoteEvent::new(10, 110, 121, 100, 0);
    let new = NoteEvent::new(10, 110, 120, 100, 0);
    let dirty = dirty_with(vec![change(1, Some(old), Some(new))]);
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));

    assert_pad_matches_expectations();
    assert_eq!(plan.index, CpuOnionIndex::Incremental);
    let Some(CpuOnionRedraw::Region(rect)) = plan.redraw else {
        panic!("单笔洋葱轨编辑必须只重绘区域，实际 {:?}", plan.redraw);
    };
    // key 120 → y = 140，key 121 → y = 120；两行 20px + 4px 余量
    assert_eq!(rect.x, 6, "x = 10 − 4");
    assert_eq!(rect.y, 116, "y = 120 − 4");
    assert_eq!(rect.width, 108, "x1 = 110 + 4");
    assert_eq!(rect.height, 48, "y1 = 160 + 4");
}

/// 余量常量与用例断言同步（改常量时这里先炸）。
fn assert_pad_matches_expectations() {
    assert!(
        (DIRTY_PAD_PX - 4.0).abs() < f32::EPSILON,
        "脏区余量改动必须同步更新用例断言"
    );
}

/// 只有脏 tick 区间（历史回放）→ 整帧重画，但索引/密度保持增量。
#[test]
fn dirty_ranges_keep_index_incremental() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let mut dirty = dirty_with(Vec::new());
    dirty.ranges.push((0, 4096));
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));
    assert_eq!(
        plan,
        super::super::CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// `dirty_tracks` 里有洋葱轨没出现在 changes 中 → 保守整帧 + 整索引。
#[test]
fn uncovered_onion_track_forces_full_rebuild() {
    let camera = camera();
    let muted = [false, false, true];
    // 轨 3 也是洋葱轨，但 changes 里没有它 → 无法证明没变
    let changed = tracks(&[1, 3]);
    let dirty = dirty_with(vec![change(
        1,
        None,
        Some(NoteEvent::new(10, 110, 121, 100, 0)),
    )]);
    assert_full_rebuild(base(&camera, &muted, Some(&dirty), Some(&changed)));
}

/// `dirty == None` 但来源点名了洋葱轨 → 无法用 changes 增量表达 → 整帧 + 整索引。
#[test]
fn dirty_none_with_onion_source_forces_full() {
    let camera = camera();
    let muted = [false, false, true];
    let onion = tracks(&[1]);
    assert_full_rebuild(base(&camera, &muted, None, Some(&onion)));
}

/// 静音轨编辑（有 changes）→ 不重绘（CPU 不画静音轨）。
#[test]
fn muted_track_edit_skips_redraw() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[2]);
    let dirty = dirty_with(vec![change(
        2,
        None,
        Some(NoteEvent::new(10, 110, 122, 100, 0)),
    )]);
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));
    assert_eq!(
        plan,
        super::super::CpuOnionPlan {
            redraw: None,
            index: CpuOnionIndex::Incremental,
        }
    );
}
