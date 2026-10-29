//! 整帧兜底路径（`plan/tests.rs` 的子模块）：冷启动 / 帧缺失 / full / 来源未知。

use lumino_editor_state::{CpuOnionChange, CpuOnionDirty};

use super::super::{CpuOnionIndex, CpuOnionPlan, CpuOnionRedraw, plan_update};
use super::{assert_full_rebuild, base, camera, dirty_with, tracks};

/// 索引缺失（冷启动）→ 整帧 + 整索引。
#[test]
fn cold_start_rebuilds_everything() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let mut input = base(&camera, &muted, None, Some(&empty));
    input.frame_ready = false;
    input.index_ready = false;
    input.gen_changed = false;
    assert_full_rebuild(input);
}

/// 帧缺失但索引还在（`clear_frame` 后恢复）：整帧即可，索引不必重建。
#[test]
fn missing_frame_keeps_existing_index() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let dirty = dirty_with(Vec::new());
    let mut input = base(&camera, &muted, Some(&dirty), Some(&empty));
    input.frame_ready = false;
    input.gen_changed = false;
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Keep,
        }
    );
}

/// `dirty.full` → 整帧 + 整索引。
#[test]
fn dirty_full_rebuilds_everything() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let dirty = CpuOnionDirty {
        full: true,
        changes: Vec::new(),
    };
    assert_full_rebuild(base(&camera, &muted, Some(&dirty), Some(&changed)));
}

/// **真正来源未知**（无脏区 + 无受影响轨集合）→ 整帧 + 整索引（保守兜底）。
#[test]
fn source_unknown_falls_back_to_full() {
    let camera = camera();
    let muted = [false, false, true];
    assert_full_rebuild(base(&camera, &muted, None, None));
}

/// `dirty_tracks == None`（来源未知）且有 changes → 保守整帧 + 整索引。
#[test]
fn unknown_dirty_tracks_forces_full_rebuild() {
    let camera = camera();
    let muted = [false, false, true];
    let note = lumino_midi_model::NoteEvent::new(10, 110, 121, 100, 0);
    let dirty = dirty_with(vec![CpuOnionChange {
        track: 1,
        old: None,
        new: Some(note),
    }]);
    assert_full_rebuild(base(&camera, &muted, Some(&dirty), None));
}
