//! 小节线/网格线 LOD 单元测试

use super::*;

fn ppq() -> f32 {
    480.0
}

fn measures_at(visible_measures: f32) -> GridLod {
    let ticks_per_measure = ppq() * 4.0;
    GridLod::compute(visible_measures * ticks_per_measure, ppq())
}

#[test]
fn test_measure_single_power_at_low_zoom() {
    let lod = measures_at(24.0);
    assert_eq!(lod.measure_count, 1);
    assert_eq!(lod.measures[0].interval, ppq() * 4.0);
    assert!(lod.measures[0].alpha > 0.99);
}

#[test]
fn test_measure_two_powers_during_transition() {
    let lod = measures_at(72.0);
    assert_eq!(lod.measure_count, 2);
    // power 0 正在淡出，power 1 全显
    assert!(lod.measures[0].alpha > 0.0 && lod.measures[0].alpha < 0.99);
    assert!(lod.measures[1].alpha > 0.99);
    assert_eq!(lod.measures[1].interval, ppq() * 4.0 * 2.0);
}

#[test]
fn test_measure_power_handoff_is_continuous() {
    let lod_at_96 = measures_at(96.0);
    let lod_at_97 = measures_at(97.0);
    // 96 处 power 0 刚好消失，power 1 全显；97 处仍然至少有 power 1
    assert_eq!(lod_at_96.measure_count, 1);
    assert!(lod_at_96.measures[0].alpha > 0.99);
    assert!(lod_at_97.measure_count >= 1);
    assert!(lod_at_97.measures[0].alpha > 0.99);
}

#[test]
fn test_grid_tiers_fade_before_half_beat() {
    // 可见小节数 10：16分网格应已消失，半拍线仍全显
    let lod = measures_at(10.0);
    let grid_16th_alpha = lod.grid_alphas[GRID_TIERS.len() - 1];
    assert_eq!(grid_16th_alpha, 0.0);
    assert!(lod.halfbeat_alpha > 0.99);
}

#[test]
fn test_half_beat_fades_before_beat() {
    // 可见小节数 18：半拍线正在淡出，拍线仍全显
    let lod = measures_at(18.0);
    assert!(lod.halfbeat_alpha > 0.0 && lod.halfbeat_alpha < 0.99);
    assert!(lod.beat_alpha > 0.99);
}
