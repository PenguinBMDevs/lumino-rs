use super::*;

#[test]
fn test_palette_manager_initialized() {
    // 确保管理器初始化不崩溃
    let mgr = &*PALETTE_MANAGER;
    assert!(!mgr.names().is_empty(), "应至少有一个调色板");
    assert!(!mgr.palettes().is_empty(), "应至少有一个已解析的调色板");
}

#[test]
fn test_default_palette_not_empty() {
    let mgr = &*PALETTE_MANAGER;
    let default = mgr.default();
    assert!(!default.colors.is_empty(), "默认调色板不能为空");
}

#[test]
fn test_default_palette_is_random() {
    // 验证 "Random" 已被冒泡为默认调色板
    let mgr = &*PALETTE_MANAGER;
    assert_eq!(mgr.default().name, "Random");
    assert_eq!(mgr.names()[0], "Random");
}

#[test]
fn test_track_color_cycling() {
    let mgr = &*PALETTE_MANAGER;
    let palette = mgr.default();
    // 验证循环取色不 panic
    for i in 0..100 {
        let _color = palette.track_color(i);
    }
}

#[test]
fn test_track_color_f32_bounds() {
    let mgr = &*PALETTE_MANAGER;
    let palette = mgr.default();
    let color = palette.track_color_f32(0);
    for &component in &color {
        assert!(
            (0.0..=1.0).contains(&component),
            "f32 颜色分量应在 0-1 范围内"
        );
    }
}

#[test]
fn test_resolve_name_valid() {
    let mgr = &*PALETTE_MANAGER;
    let name = mgr.names()[0];
    let resolved = mgr.resolve_name(name);
    assert_eq!(resolved, name);
}

#[test]
fn test_resolve_name_invalid_falls_back() {
    let mgr = &*PALETTE_MANAGER;
    let resolved = mgr.resolve_name("nonexistent_palette");
    assert_eq!(resolved, mgr.names()[0]);
}

#[test]
fn test_get_valid_palette() {
    let mgr = &*PALETTE_MANAGER;
    let name = mgr.names()[0];
    assert!(mgr.get(name).is_some());
}

#[test]
fn test_get_invalid_palette() {
    let mgr = &*PALETTE_MANAGER;
    assert!(mgr.get("nonexistent_palette").is_none());
}

#[test]
fn test_decode_random_png() {
    // 验证我们能在测试中解码已知的调色板
    let mgr = &*PALETTE_MANAGER;
    for palette in mgr.palettes() {
        assert!(
            palette.colors.len() >= 8,
            "调色板 '{}' 应有至少 8 种颜色，实际 {}",
            palette.name,
            palette.colors.len()
        );
    }
}

// ─── 锁机制测试 ───────────────────────────────────────────────────────────────

#[test]
fn test_lock_unlock_palette() {
    // 初始状态为解锁
    unlock_palette();
    assert!(!is_palette_locked(), "初始应为解锁");

    lock_palette();
    assert!(is_palette_locked(), "锁定后应为 true");

    unlock_palette();
    assert!(!is_palette_locked(), "解锁后应为 false");
}

#[test]
fn test_set_palette_ignored_when_locked() {
    // 先设置为已知调色板，再锁定
    let mgr = &*PALETTE_MANAGER;
    let first_name = mgr.names()[0];
    let second_name = if mgr.names().len() > 1 {
        mgr.names()[1]
    } else {
        // 只有一个调色板时跳过
        return;
    };

    // 解锁并设置到一个非默认调色板
    unlock_palette();
    let palette_set = set_current_palette_by_name(second_name);
    assert!(palette_set, "未锁定时应成功切换");
    assert_eq!(current_palette_name(), second_name);

    // 锁定后再尝试切换
    lock_palette();
    let palette_set = set_current_palette_by_name(first_name);
    assert!(!palette_set, "锁定时应返回 false");
    // 确认当前调色板未改变
    assert_eq!(
        current_palette_name(),
        second_name,
        "锁定时调色板不应被修改"
    );
}

// ─── 洋葱皮偏移测试 ────────────────────────────────────────────────────────────

#[test]
fn test_onion_track_color_differs_from_main() {
    // 当调色板有至少 1 种颜色时，
    // onion_track_color(0) 与主音轨蓝色固定色不同（onion 取调色板第一色，主音轨为固定蓝）
    let mgr = &*PALETTE_MANAGER;
    let palette = mgr.default();
    if palette.colors.is_empty() {
        return;
    }

    unlock_palette();
    set_current_palette_by_name(palette.name);

    let onion_color = onion_track_color(0);

    // onion 从调色板索引 0 开始取色（offset = 0）
    assert_eq!(onion_color, palette.colors[0], "onion 应取调色板第一个颜色");
}

#[test]
fn test_onion_track_color_offset_is_one() {
    // 验证 onion_track_color(i) 对应 palette[i % len]
    // 而非 palette[(1 + i) % len]
    let mgr = &*PALETTE_MANAGER;
    let palette = mgr.default();
    if palette.colors.is_empty() {
        return;
    }

    unlock_palette();
    set_current_palette_by_name(palette.name);

    for i in 0..palette.colors.len().min(10) {
        let expected = palette.colors[i % palette.colors.len()];
        assert_eq!(
            onion_track_color(i),
            expected,
            "onion_track_color({}) 应等于 palette[{} % {}]",
            i,
            i,
            palette.colors.len()
        );
    }
}

#[test]
fn test_onion_track_color_cycling() {
    // 验证大量洋葱皮取色不 panic
    set_current_palette_by_name(default_palette_name());
    for i in 0..100 {
        let _color = onion_track_color(i);
    }
}

#[test]
fn test_onion_track_color_f32_bounds() {
    set_current_palette_by_name(default_palette_name());
    let color = onion_track_color_f32(0);
    for &component in &color {
        assert!(
            (0.0..=1.0).contains(&component),
            "f32 颜色分量应在 0-1 范围内"
        );
    }
}

// ─── BUG 回归测试 ─────────────────────────────────────────────────────────────

#[test]
fn test_unlock_after_lock_allows_palette_switch() {
    // BUG 回归：MIDI 文件关闭后调色板仍然无法调整
    // 场景模拟：加载 MIDI（锁定）→ 关闭 MIDI（解锁）→ 应该能切换调色板
    let mgr = &*PALETTE_MANAGER;
    if mgr.names().len() < 2 {
        return;
    }
    let name_a = mgr.names()[0];
    let name_b = mgr.names()[1];

    // 1. 锁状态等同于 MIDI 加载后
    unlock_palette();
    set_current_palette_by_name(name_b);
    lock_palette();
    assert!(is_palette_locked());
    assert_eq!(current_palette_name(), name_b);

    // 2. 解锁后（模拟 Close / New 操作）应该能切换调色板
    unlock_palette();
    assert!(!is_palette_locked());
    let palette_set = set_current_palette_by_name(name_a);
    assert!(palette_set, "解锁后应能切换调色板");
    assert_eq!(current_palette_name(), name_a);
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

fn default_palette_name() -> &'static str {
    PALETTE_MANAGER.default().name
}
