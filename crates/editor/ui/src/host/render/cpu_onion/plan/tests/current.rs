//! 布局变化 / 切轨 / 当前轨直接编辑 / 复用（`plan/tests.rs` 的子模块）。

use lumino_midi_model::NoteEvent;

use super::super::{CpuOnionIndex, CpuOnionPlan, CpuOnionRedraw, plan_update};
use super::{base, camera, change, dirty_with, tracks};

/// 布局键变化（滚动）：整帧但不重建索引。
#[test]
fn layout_change_redraws_full_without_rebuilding_index() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let mut input = base(&camera, &muted, None, Some(&empty));
    input.layout_changed = true;
    input.gen_changed = false;
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Keep,
        }
    );
}

/// 切轨（布局变化 + 代次变化 + `Some(empty)` 来源）= 拖动后立刻切轨的真实序列：
/// 整帧，索引只做增量补齐（不重建整份）。
#[test]
fn current_track_switch_redraws_full_and_repairs_index_incrementally() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let dirty = dirty_with(Vec::new());
    let mut input = base(&camera, &muted, Some(&dirty), Some(&empty));
    input.layout_changed = true;
    input.current_track = 1;
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 切轨时若「旧当前轨」出现在来源集合里却没有原子变更记录 → 无法证明它变了什么，
/// 保守整帧 + 整索引（提示词约定的兜底分支）。
#[test]
fn switch_with_unrecorded_onion_track_falls_back_to_full() {
    let camera = camera();
    let muted = [false, false, true];
    let current = tracks(&[0]);
    let dirty = dirty_with(Vec::new());
    let mut input = base(&camera, &muted, Some(&dirty), Some(&current));
    input.layout_changed = true;
    // 切到轨 1 后，轨 0 变成洋葱轨且有未记录的编辑
    input.current_track = 1;
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Rebuild,
        }
    );
}

/// 切轨后旧当前轨几何过期（代次未变）→ 整帧 + 索引增量补齐。
#[test]
fn stale_track_repair_forces_incremental_index() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let mut input = base(&camera, &muted, None, Some(&empty));
    input.layout_changed = true;
    input.gen_changed = false;
    input.stale_needs_repair = true;
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 代次未变且无脏区 → 复用。
#[test]
fn nothing_changed_reuses_frame() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let mut input = base(&camera, &muted, None, Some(&changed));
    input.gen_changed = false;
    let plan = plan_update(input);
    assert_eq!(plan, CpuOnionPlan::REUSE);
    assert!(plan.redraw.is_none(), "复用帧不得重绘");
}

/// **Host 实际形态**：当前轨拖动 → `dirty == None`（拖动不经记录点）
/// + `onion_dirty_tracks == Some(empty)` → 不重绘、索引只增量刷新。
///
/// 这是 Q4 B「当前轨编辑不重绘 CPU 帧」的主路径：`dirty == None` **不等于**
/// 「来源未知」，它和 `dirty_tracks` 一起才定义来源。
#[test]
fn host_drag_shape_without_dirty_skips_redraw() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let plan = plan_update(base(&camera, &muted, None, Some(&empty)));
    assert_eq!(
        plan,
        CpuOnionPlan {
            redraw: None,
            index: CpuOnionIndex::Incremental,
        },
        "当前轨拖动（dirty=None + Some(empty)）不得整帧重绘"
    );
}

/// 同形态但来源点名当前轨（`Some({current})`）→ 同样不重绘。
#[test]
fn host_drag_shape_current_track_source_skips_redraw() {
    let camera = camera();
    let muted = [false, false, true];
    let current = tracks(&[0]);
    let plan = plan_update(base(&camera, &muted, None, Some(&current)));
    assert_eq!(
        plan,
        CpuOnionPlan {
            redraw: None,
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 同形态但来源点名静音轨 → 不重绘（CPU 不画静音轨）。
#[test]
fn host_drag_shape_muted_source_skips_redraw() {
    let camera = camera();
    let muted = [false, false, true];
    let muted_track = tracks(&[2]);
    let plan = plan_update(base(&camera, &muted, None, Some(&muted_track)));
    assert_eq!(
        plan,
        CpuOnionPlan {
            redraw: None,
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 当前轨编辑（原子变更列表为空但脏区对象存在）→ 不重绘，只更新快照/索引。
#[test]
fn current_track_only_edit_skips_redraw() {
    let camera = camera();
    let muted = [false, false, true];
    let current = tracks(&[0]);
    let dirty = dirty_with(Vec::new());
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&current)));
    assert_eq!(
        plan,
        CpuOnionPlan {
            redraw: None,
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 当前轨 + 洋葱轨同时变化：只按洋葱轨算区域，当前轨仍被排除。
#[test]
fn mixed_changes_ignore_current_track_geometry() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[0, 1]);
    let dirty = dirty_with(vec![
        // 当前轨（0）：渲染上必须被忽略，连脏区都不该参与
        change(0, None, Some(NoteEvent::new(0, 2000, 100, 100, 0))),
        change(1, None, Some(NoteEvent::new(10, 110, 121, 100, 0))),
    ]);
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));
    let Some(CpuOnionRedraw::Region(rect)) = plan.redraw else {
        panic!("应只按洋葱轨算脏区，实际 {:?}", plan.redraw);
    };
    assert_eq!(rect.x, 6);
    assert_eq!(rect.width, 108);
}
