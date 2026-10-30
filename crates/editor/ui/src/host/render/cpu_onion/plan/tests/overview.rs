//! 总览 LOD 的重绘放大（`plan/tests.rs` 子模块）。

use lumino_cpu_onion::CpuOnionRect;

use super::super::{CpuOnionRedraw, overview_redraw};

fn region() -> CpuOnionRedraw {
    CpuOnionRedraw::Region(CpuOnionRect::new(1, 2, 3, 4))
}

#[test]
fn overview_promotes_region_to_full() {
    assert_eq!(
        overview_redraw(true, false, false, Some(region())),
        Some(CpuOnionRedraw::Full),
        "总览档单音符脏区覆盖不了密度整列，必须整帧"
    );
}

#[test]
fn overview_promotes_gen_change_without_visual_dirty_to_full() {
    assert_eq!(
        overview_redraw(true, false, true, None),
        Some(CpuOnionRedraw::Full),
        "当前轨/静音轨编辑也会改全曲密度统计，代次一变必须整帧"
    );
}

#[test]
fn overview_keeps_reuse_without_changes() {
    assert_eq!(
        overview_redraw(true, false, false, None),
        None,
        "没变化时不得每帧重画"
    );
}

#[test]
fn mode_change_forces_full_even_without_dirty_area() {
    assert_eq!(
        overview_redraw(false, true, false, None),
        Some(CpuOnionRedraw::Full),
        "退出总览必须把蓝色密度层换回 Exact 音符"
    );
    assert_eq!(
        overview_redraw(true, true, false, None),
        Some(CpuOnionRedraw::Full),
        "进入总览必须立即换成密度层"
    );
    assert_eq!(
        overview_redraw(false, true, true, Some(region())),
        Some(CpuOnionRedraw::Full),
        "跨档优先于区域脏区"
    );
}

#[test]
fn exact_mode_keeps_original_plan() {
    let dirty = Some(region());
    assert_eq!(overview_redraw(false, false, true, dirty), dirty);
    assert_eq!(overview_redraw(false, false, false, None), None);
    assert_eq!(
        overview_redraw(true, false, true, Some(CpuOnionRedraw::Full)),
        Some(CpuOnionRedraw::Full)
    );
}
