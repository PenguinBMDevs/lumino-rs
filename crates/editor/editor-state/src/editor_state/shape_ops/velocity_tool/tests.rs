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
