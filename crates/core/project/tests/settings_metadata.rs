//! `metadata.toml` 的 `[settings]` 段序列化契约（洋葱皮显示范围的工程范围记忆）。
//!
//! 放在集成测试里而不是 `metadata.rs` 内联模块：`metadata.rs` 已 391 行（REF-001
//! 上限 400），内联加 60 行测试会超限；这里只用公开 API，正好当外部契约测试。

use lumino_project::{ProjectMetadata, SettingsMetadata};

/// 洋葱皮显示范围必须能写进 TOML 并原样读回（工程范围记忆的存储层）。
#[test]
fn test_onion_skin_mode_roundtrips_through_toml() {
    let mut meta = ProjectMetadata::default_with_name("Onion");
    meta.settings = Some(SettingsMetadata {
        onion_skin_mode: Some("previous_track".into()),
        ..Default::default()
    });

    let toml = meta.to_toml_str().expect("序列化 metadata.toml 失败");
    assert!(
        toml.contains("onion_skin_mode = \"previous_track\""),
        "settings 段必须写出 onion_skin_mode（实得：\n{toml}）"
    );

    let back = ProjectMetadata::from_toml_str(&toml).expect("解析 metadata.toml 失败");
    assert_eq!(
        back.settings
            .as_ref()
            .and_then(|s| s.onion_skin_mode.as_deref()),
        Some("previous_track"),
        "读回后洋葱皮显示范围必须一致"
    );
}

/// 老工程（没有 `[settings]` 段）必须零差异：不写该键，读回仍是 `None`。
///
/// 「读回 None」在加载侧等价于「用默认范围（全部音轨）」——正是加这个字段前的行为。
#[test]
fn test_old_project_without_settings_stays_none() {
    let meta = ProjectMetadata::default_with_name("Old");
    assert!(meta.settings.is_none(), "新工程默认不写 settings 段");

    let toml = meta.to_toml_str().expect("序列化 metadata.toml 失败");
    assert!(
        !toml.contains("onion_skin_mode"),
        "未设置时不得写出该键（否则老版本工程会被写入一个自己不认识的值）"
    );

    let back = ProjectMetadata::from_toml_str(&toml).expect("解析 metadata.toml 失败");
    assert!(
        back.settings.is_none(),
        "无 [settings] 段的老工程读回后 settings 必须仍是 None"
    );
}

/// 只写一项时其余字段必须保持 `None`（不越权接管别人的设置项）。
#[test]
fn test_settings_only_carries_onion_skin_mode() {
    let settings = SettingsMetadata {
        onion_skin_mode: Some("next_track".into()),
        ..Default::default()
    };
    assert!(settings.theme.is_none());
    assert!(settings.synth_backend.is_none());
    assert!(settings.soundfont_path.is_none());
    assert!(settings.auto_scroll_mode.is_none());
    assert!(settings.enable_256key.is_none());
    assert!(settings.velocity_filter_threshold.is_none());
}
