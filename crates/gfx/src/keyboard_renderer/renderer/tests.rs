use super::super::{KeyInstance, KeyboardViewportUniform};
use super::KeyboardRenderer;
use crate::KeyboardPrepareParams;

#[test]
fn test_key_instance_creation() {
    let instance =
        KeyInstance::new([10.0, 20.0], [60.0, 20.0], [1.0, 1.0, 1.0, 1.0], false, 60);

    assert_eq!(instance.position, [10.0, 20.0]);
    assert_eq!(instance.size, [60.0, 20.0]);
    assert_eq!(instance.is_black, 0.0);
    assert_eq!(instance.key_index, 60.0);
}

#[test]
fn test_is_key_dark() {
    // C (0) = 白键
    assert!(!KeyboardRenderer::is_key_dark(0));
    // C# (1) = 黑键
    assert!(KeyboardRenderer::is_key_dark(1));
    // D (2) = 白键
    assert!(!KeyboardRenderer::is_key_dark(2));
    // D# (3) = 黑键
    assert!(KeyboardRenderer::is_key_dark(3));
    // E (4) = 白键
    assert!(!KeyboardRenderer::is_key_dark(4));
    // F (5) = 白键
    assert!(!KeyboardRenderer::is_key_dark(5));
    // F# (6) = 黑键
    assert!(KeyboardRenderer::is_key_dark(6));
}

#[test]
fn test_viewport_uniform_creation() {
    let uniform = KeyboardViewportUniform::from_params(&KeyboardPrepareParams {
        viewport_size: (1920.0, 1080.0),
        keyboard_width: 60.0,
        ruler_height: 30.0,
        scroll_y: 100.0,
        zoom_y: 20.0,
        visible_key_count: 128,
    });

    assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
    assert_eq!(uniform.keyboard_width, 60.0);
    assert_eq!(uniform.ruler_height, 30.0);
    assert_eq!(uniform.scroll_y, 100.0);
    assert_eq!(uniform.zoom_y, 20.0);
    assert_eq!(uniform.visible_key_count, 128.0);
}
