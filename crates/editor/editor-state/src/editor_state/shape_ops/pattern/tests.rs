//! Formula / Pattern 几何测试

use std::collections::BTreeMap;

use super::*;
use lumino_core::shape::PatternParams;
use lumino_core::shape::pattern_presets::{PATTERN_PRESETS, pattern_from_preset};

fn sine(loops: f32) -> PatternParams {
    PatternParams {
        formula: "sin(2*pi*x)".into(),
        vars: BTreeMap::new(),
        loops,
        k: 1.0,
        ..PatternParams::default()
    }
}

fn path() -> Vec<Pt> {
    vec![(0.0, 60.0), (480.0, 60.0)]
}

#[test]
fn test_sine_wave_peaks_up() {
    let out = formed_path(&path(), &sine(1.0), 480.0);
    assert!(out.len() > LOOP_SAMPLES, "应有一圈采样");
    let peak = out
        .iter()
        .min_by(|a, b| {
            (a.0 - 120.0)
                .abs()
                .partial_cmp(&(b.0 - 120.0).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
        .expect("应有输出");
    assert!((peak.0 - 120.0).abs() < 2.0, "波峰应在 x=0.25（tick 120）");
    assert!(
        (peak.1 - 61.0).abs() < 1e-6,
        "时间向前的直线上，y>0 应在上方"
    );
}

#[test]
fn test_loops_repeat_three_times() {
    let out = formed_path(&path(), &sine(3.0), 480.0);
    let peaks = out.iter().filter(|p| p.1 > 60.9).count();
    assert!(peaks >= 3, "三圈应出现三个正向波峰区，实际 {peaks}");
    let max_x = out.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    assert!(max_x <= 480.0 + 1e-6, "不应越过曲线终点");
}

#[test]
fn test_mirror_flips_side() {
    let mut params = sine(1.0);
    params.mirror = true;
    let out = formed_path(&path(), &params, 480.0);
    let min_y = out.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    assert!((min_y - 59.0).abs() < 1e-6, "mirror 应在曲线下方");
}

#[test]
fn test_along_moves_points_forward() {
    let params = PatternParams {
        formula: "0".into(),
        vars: BTreeMap::new(),
        along: Some("0.25".into()),
        loops: 1.0,
        k: 1.0,
        ..PatternParams::default()
    };
    let out = formed_path(&path(), &params, 480.0);
    let min_x = out.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = out.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    assert!(min_x >= 120.0 - 1e-6, "along=0.25 应整体前移");
    assert!(max_x <= 480.0 + 1e-6, "图案裁在曲线段内");
    assert!(max_x > 479.0, "前移后仍应铺到曲线末端");
}

#[test]
fn test_invalid_formula_returns_original() {
    let params = PatternParams {
        formula: "sin(".into(),
        vars: BTreeMap::new(),
        ..PatternParams::default()
    };
    assert_eq!(formed_path(&path(), &params, 480.0), path());
}

#[test]
fn test_point_limit_truncates() {
    let out = formed_path(&path(), &sine(2000.0), 480.0);
    assert!(out.len() <= MAX_POINTS);
    assert!(out.len() > 1000);
}

#[test]
fn test_all_presets_generate() {
    for preset in PATTERN_PRESETS {
        let params = pattern_from_preset(preset.id, 1.0, 1.0)
            .unwrap_or_else(|| panic!("预设不存在：{}", preset.id));
        let out = formed_path(&path(), &params, 480.0);
        assert!(out.len() > 2, "预设 {} 应生成点列", preset.id);
    }
}

#[test]
fn test_loop_curve_takes_precedence() {
    let params = PatternParams {
        formula: "999".into(),
        vars: BTreeMap::new(),
        loops: 1.0,
        k: 1.0,
        loop_curve: Some(lumino_core::shape::LoopCurve {
            pts: vec![[0.0, 0.0], [0.5, 2.0], [1.0, 0.0]],
            sharp: Vec::new(),
        }),
        ..PatternParams::default()
    };
    let out = formed_path(&path(), &params, 480.0);
    let max_y = out.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    assert!((max_y - 62.0).abs() < 1e-6, "手改环应优先于公式");
}
