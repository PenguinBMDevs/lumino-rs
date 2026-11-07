//! 力度工具测试

use super::*;
use lumino_core::shape::VelocityEnv;

fn note(tick: f32, length: f32) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key: 60,
        length,
        velocity: 100,
        channel: 0,
    }
}

#[test]
fn test_linear_creates_slope() {
    let mut env = VelocityEnv::default();
    apply_vel_tool(
        &mut env,
        VelTool::Linear,
        &Drag::new((0.0, 20.0), (1.0, 100.0)),
        480,
    );
    assert_eq!(env_at(&env, 0.5, false), 60);
}

#[test]
fn test_curve_has_samples_and_endpoints() {
    let mut env = VelocityEnv::default();
    let points = apply_vel_tool(
        &mut env,
        VelTool::Curve,
        &Drag::new((0.0, 40.0), (1.0, 80.0)),
        480,
    );
    assert_eq!(points.len(), CURVE_STEPS + 1);
    assert_eq!(points[0][1], 40.0);
    assert_eq!(points[points.len() - 1][1], 80.0);
}

#[test]
fn test_formula_generates_finite_points() {
    let mut env = VelocityEnv::default();
    let points = apply_vel_tool(
        &mut env,
        VelTool::Formula,
        &Drag {
            formula: Some("50 + 20 * sin(x * 2 * pi * 4)".into()),
            ..Drag::new((0.0, 0.0), (1.0, 0.0))
        },
        480,
    );
    assert_eq!(points.len(), FORMULA_POINTS);
    assert!(points.iter().flatten().all(|value| value.is_finite()));
}

#[test]
fn test_env_applies_over_note_span() {
    let env = VelocityEnv::new(vec![[0.0, 20.0], [1.0, 100.0]]);
    let mut notes = vec![note(0.0, 100.0), note(100.0, 0.0)];
    apply_env_to_notes(&mut notes, &env);
    assert_eq!(notes[0].velocity, 20);
    assert_eq!(notes[1].velocity, 100);
}

#[test]
fn test_env_follows_shape_translation() {
    let env = VelocityEnv::new(vec![[0.0, 20.0], [1.0, 100.0]]);
    let mut notes = vec![note(0.0, 100.0), note(100.0, 0.0)];
    apply_env_to_notes(&mut notes, &env);
    let before: Vec<u8> = notes.iter().map(|note| note.velocity).collect();

    for note in &mut notes {
        note.tick += 1000.0;
    }
    apply_env_to_notes(&mut notes, &env);
    let after: Vec<u8> = notes.iter().map(|note| note.velocity).collect();
    assert_eq!(before, after, "整体平移后同一 u 的力度必须不变");
}

#[test]
fn test_pencil_produces_legal_envelope() {
    let mut env = VelocityEnv::default();
    let points = apply_vel_tool(
        &mut env,
        VelTool::Pencil,
        &Drag::new((0.1, 20.0), (0.9, 100.0)),
        480,
    );
    assert_eq!(points.len(), 17, "Pencil 固定采样 16 段");
    assert!(
        points.windows(2).all(|pair| pair[0][0] <= pair[1][0]),
        "u 必须单调"
    );
    assert!(
        points.iter().all(|point| (1.0..=127.0).contains(&point[1])),
        "力度必须落在 1..=127"
    );
    assert_eq!(points.first().map(|point| point[1]), Some(20.0));
    assert_eq!(points.last().map(|point| point[1]), Some(100.0));
}

#[test]
fn test_formula_loops_produce_periods() {
    let mut env = VelocityEnv::default();
    let points = apply_vel_tool(
        &mut env,
        VelTool::Formula,
        &Drag {
            preset: Some("wave".into()),
            loops: 4.0,
            ..Drag::new((0.0, 0.0), (1.0, 0.0))
        },
        480,
    );
    let mut peaks = 0;
    let mut rising = false;
    for pair in points.windows(2) {
        if pair[1][1] > pair[0][1] {
            rising = true;
        } else if pair[1][1] < pair[0][1] && rising {
            peaks += 1;
            rising = false;
        }
    }
    assert_eq!(peaks, 4, "loops=4 应有 4 个极大值");
}

#[test]
fn test_formula_loops_clamped_to_most() {
    let drag = Drag {
        loops: VEL_MOST_LOOPS * 2.0,
        ..Drag::new((0.0, 0.0), (1.0, 0.0))
    };
    assert_eq!(drag.effective_loops(), VEL_MOST_LOOPS);

    let start = std::time::Instant::now();
    let points = apply_vel_tool(&mut VelocityEnv::default(), VelTool::Formula, &drag, 480);
    assert_eq!(points.len(), FORMULA_POINTS);
    assert!(
        points.iter().flatten().all(|value| value.is_finite()),
        "超高圈数也必须产出有限值"
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "圈数钳制后不应卡死"
    );
}
