//! 力度包络测试

use super::*;

#[test]
fn test_interpolates_and_empty_default() {
    let env = VelocityEnv {
        points: vec![[0.0, 40.0], [1.0, 100.0]],
    };
    assert_eq!(env_at(&env, 0.5, false), 70);
    assert_eq!(env_at(&VelocityEnv::default(), 0.5, false), 100);
}

#[test]
fn test_same_u_jump_takes_right_or_left() {
    let env = VelocityEnv {
        points: vec![[0.5, 20.0], [0.5, 90.0], [1.0, 100.0]],
    };
    assert_eq!(env_at(&env, 0.5, false), 90, "默认取右侧");
    assert_eq!(env_at(&env, 0.5, true), 20, "left 取左侧");
}

#[test]
fn test_tidy_sorts_and_clamps() {
    let mut env = VelocityEnv {
        points: vec![[2.0, 200.0], [-1.0, 0.0], [0.5, 50.0]],
    };
    tidy_env(&mut env);
    assert_eq!(env.points, vec![[0.0, 1.0], [0.5, 50.0], [1.0, 127.0]]);
}

#[test]
fn test_paint_replaces_range_and_keeps_boundaries() {
    let mut env = VelocityEnv {
        points: vec![[0.0, 20.0], [0.25, 80.0], [0.75, 60.0], [1.0, 100.0]],
    };
    paint_env(&mut env, 0.25, 0.75, 40);
    assert_eq!(env_at(&env, 0.5, false), 40);
    assert_eq!(env_at(&env, 0.0, false), 20);
    assert_eq!(env_at(&env, 1.0, false), 100);
}
