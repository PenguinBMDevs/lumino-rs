//! Pattern 预设测试

use super::*;

#[test]
fn test_all_presets_are_valid() {
    assert!(PATTERN_PRESETS.len() >= 6);
    let mut ids = std::collections::BTreeSet::new();
    for preset in PATTERN_PRESETS {
        assert!(ids.insert(preset.id), "预设 id 重复：{}", preset.id);
        let params = pattern_from_preset(preset.id, 1.0, 1.0)
            .unwrap_or_else(|| panic!("预设不存在：{}", preset.id));
        params
            .validate()
            .unwrap_or_else(|e| panic!("预设 {} 非法：{e}", preset.id));
    }
}

#[test]
fn test_unknown_preset_is_none() {
    assert!(pattern_from_preset("no-such-preset", 1.0, 1.0).is_none());
}
