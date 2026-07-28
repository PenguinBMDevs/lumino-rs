//! KeyboardRenderer 集成测试

use lumino_gfx::{KeyInstance, KeyboardPrepareParams, KeyboardViewportUniform};

/// 测试 KeyboardViewportUniform 内存布局
#[test]
fn test_keyboard_viewport_uniform_layout() {
    let _uniform = KeyboardViewportUniform::from_params(&KeyboardPrepareParams {
        viewport_size: (1920.0, 1080.0),
        keyboard_width: 60.0,
        ruler_height: 30.0,
        scroll_y: 100.0,
        zoom_y: 20.0,
        visible_key_count: 128,
    });

    // 验证大小（实际大小可能因对齐而变化）
    let size = std::mem::size_of::<KeyboardViewportUniform>();
    assert!((32..=48).contains(&size), "Unexpected size: {}", size);

    // 验证对齐
    assert_eq!(std::mem::align_of::<KeyboardViewportUniform>(), 4);
}

/// 测试 KeyInstance 内存布局
#[test]
fn test_key_instance_layout() {
    let _instance = KeyInstance::new([10.0, 20.0], [60.0, 20.0], [1.0, 1.0, 1.0, 1.0], false, 60);

    // 验证大小 (4 * 2 + 4 * 2 + 4 * 4 + 4 + 4 + 4 * 2 = 48)
    assert_eq!(std::mem::size_of::<KeyInstance>(), 48);

    // 验证对齐
    assert_eq!(std::mem::align_of::<KeyInstance>(), 4);
}

/// 测试黑键判断逻辑
#[test]
fn test_black_key_detection() {
    // C (0) = 白键
    assert!(!is_key_dark(0));
    // C# (1) = 黑键
    assert!(is_key_dark(1));
    // D (2) = 白键
    assert!(!is_key_dark(2));
    // D# (3) = 黑键
    assert!(is_key_dark(3));
    // E (4) = 白键
    assert!(!is_key_dark(4));
    // F (5) = 白键
    assert!(!is_key_dark(5));
    // F# (6) = 黑键
    assert!(is_key_dark(6));
    // G (7) = 白键
    assert!(!is_key_dark(7));
    // G# (8) = 黑键
    assert!(is_key_dark(8));
    // A (9) = 白键
    assert!(!is_key_dark(9));
    // A# (10) = 黑键
    assert!(is_key_dark(10));
    // B (11) = 白键
    assert!(!is_key_dark(11));

    // 测试跨八度
    assert!(!is_key_dark(12)); // C
    assert!(is_key_dark(13)); // C#
}

fn is_key_dark(key: isize) -> bool {
    let note_in_octave = key.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 测试琴键位置计算
#[test]
fn test_key_position_calculation() {
    let zoom_y = 20.0;
    let scroll_y = 0.0;
    let ruler_height = 30.0;
    let max_key_index = 127.0;

    // 测试第 60 个键（C4）的位置
    let key_index = 60;
    let world_y = (max_key_index - key_index as f32) * zoom_y;
    let screen_y = world_y - scroll_y + ruler_height;

    assert_eq!(world_y, 1340.0); // (127 - 60) * 20
    assert_eq!(screen_y, 1370.0); // 1340 + 30
}

/// 测试大量琴键实例生成性能
#[test]
fn test_key_instance_generation_performance() {
    use std::time::Instant;

    let visible_key_count = 128;
    let keyboard_width = 60.0;
    let zoom_y = 20.0;
    let scroll_y = 0.0;
    let ruler_height = 30.0;
    let max_key_index = (visible_key_count - 1) as f32;

    let start = Instant::now();

    let mut instances = Vec::with_capacity(visible_key_count as usize);

    for i in 0..visible_key_count {
        let key_index = i as isize;
        let world_y = (max_key_index - key_index as f32) * zoom_y;
        let screen_y = world_y - scroll_y + ruler_height;

        if screen_y + zoom_y < ruler_height || screen_y > 10000.0 {
            continue;
        }

        let note_in_octave = key_index.rem_euclid(12);
        let is_black = matches!(note_in_octave, 1 | 3 | 6 | 8 | 10);

        let color = if is_black {
            [0.2, 0.2, 0.2, 1.0]
        } else {
            [0.9, 0.9, 0.9, 1.0]
        };

        let key_width = if is_black {
            keyboard_width * 0.6
        } else {
            keyboard_width
        };

        let x_offset = if is_black { keyboard_width * 0.4 } else { 0.0 };

        instances.push(KeyInstance::new(
            [x_offset, screen_y],
            [key_width, zoom_y],
            color,
            is_black,
            i,
        ));
    }

    let elapsed = start.elapsed();
    println!(
        "Generated {} key instances in {:?}",
        instances.len(),
        elapsed
    );

    // 性能要求：生成 128 个琴键实例应该在 1ms 以内
    assert!(
        elapsed.as_micros() < 1000,
        "Key generation too slow: {:?}",
        elapsed
    );
}
