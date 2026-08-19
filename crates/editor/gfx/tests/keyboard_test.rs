//! 键盘逻辑集成测试
//!
//! `KeyboardRenderer`（wgpu 管线）与 `KeyInstance` 已随未接线的键盘渲染系统删除，
//! 此处保留活跃的 `is_black_key` 逻辑测试。

use lumino_gfx::is_black_key;

/// 测试黑键判断逻辑
#[test]
fn test_black_key_detection() {
    // C (0) = 白键
    assert!(!is_black_key(0));
    // C# (1) = 黑键
    assert!(is_black_key(1));
    // D (2) = 白键
    assert!(!is_black_key(2));
    // D# (3) = 黑键
    assert!(is_black_key(3));
    // E (4) = 白键
    assert!(!is_black_key(4));
    // F (5) = 白键
    assert!(!is_black_key(5));
    // F# (6) = 黑键
    assert!(is_black_key(6));
    // G (7) = 白键
    assert!(!is_black_key(7));
    // G# (8) = 黑键
    assert!(is_black_key(8));
    // A (9) = 白键
    assert!(!is_black_key(9));
    // A# (10) = 黑键
    assert!(is_black_key(10));
    // B (11) = 白键
    assert!(!is_black_key(11));

    // 测试跨八度
    assert!(!is_black_key(12)); // C
    assert!(is_black_key(13)); // C#
}
