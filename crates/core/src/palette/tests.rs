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
fn test_track_color_cycling() {
    let mgr = &*PALETTE_MANAGER;
    let p = mgr.default();
    // 验证循环取色不 panic
    for i in 0..100 {
        let _color = p.track_color(i);
    }
}

#[test]
fn test_track_color_f32_bounds() {
    let mgr = &*PALETTE_MANAGER;
    let p = mgr.default();
    let c = p.track_color_f32(0);
    for &component in &c {
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
