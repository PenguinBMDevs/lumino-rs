//! EditMode 单元测试

use crate::velocity::EditMode;

#[test]
fn test_edit_mode_default_is_velocity() {
    let mode = EditMode::default();
    assert_eq!(mode, EditMode::Velocity);
}

#[test]
fn test_edit_mode_is_cc() {
    assert!(!EditMode::Velocity.is_cc());
    assert!(EditMode::Cc(1).is_cc());
    assert!(!EditMode::Tempo.is_cc());
}

#[test]
fn test_edit_mode_is_tempo() {
    assert!(!EditMode::Velocity.is_tempo());
    assert!(!EditMode::Cc(1).is_tempo());
    assert!(EditMode::Tempo.is_tempo());
}

#[test]
fn test_edit_mode_display_name() {
    assert_eq!(EditMode::Velocity.display_name(), "力度");
    assert_eq!(EditMode::Tempo.display_name(), "速度");
    assert_eq!(EditMode::Cc(1).display_name(), "CC");
}
