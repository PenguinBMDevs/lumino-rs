//! 纯水平增量滚动（`plan/tests.rs` 子模块）。

use lumino_midi_model::NoteEvent;

use super::super::{CpuOnionIndex, CpuOnionPlan, CpuOnionRedraw, plan_update};
use super::{base, camera, change, dirty_with, tracks};

/// 只有相机 `scroll_x` 变、位移为整数 → `Scroll`，索引 Keep。
#[test]
fn pure_horizontal_scroll_plans_incremental_strip() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let mut input = base(&camera, &muted, None, Some(&empty));
    input.layout_changed = true;
    input.gen_changed = false;
    input.scroll_dx = Some(4);
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Scroll(4)),
            index: CpuOnionIndex::Keep,
        }
    );
}

/// 与滚动同帧的文档变更：不得只画窄条，退回整帧。
#[test]
fn scroll_with_document_change_falls_back_to_full() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let dirty = dirty_with(vec![change(
        1,
        None,
        Some(NoteEvent::new(20, 30, 60, 100, 0)),
    )]);
    let mut input = base(&camera, &muted, Some(&dirty), Some(&empty));
    input.layout_changed = true;
    input.gen_changed = true;
    input.scroll_dx = Some(4);
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Incremental,
        }
    );
}

/// 位移 0 不是滚动增量：落回布局分支整帧（防御纯函数误用）。
#[test]
fn zero_scroll_dx_does_not_plan_incremental() {
    let camera = camera();
    let muted = [false, false, true];
    let empty = tracks(&[]);
    let mut input = base(&camera, &muted, None, Some(&empty));
    input.layout_changed = true;
    input.gen_changed = false;
    input.scroll_dx = Some(0);
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Keep,
        }
    );
}
