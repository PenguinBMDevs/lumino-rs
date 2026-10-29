//! 自动保存配置字段测试（独立文件，避免主配置模块继续膨胀）。

use super::*;

/// 自动保存默认关闭：全新安装不改变用户现有行为。
#[test]
fn test_autosave_defaults_to_disabled() {
    let config = UiConfig::default();
    assert!(!config.autosave_enabled, "自动保存开关默认应为关闭");
    assert_eq!(
        config.autosave_interval_mins, 5,
        "自动保存间隔默认应为 5 分钟"
    );
    assert_eq!(
        config.effective_autosave_interval_mins(),
        5,
        "默认间隔经 clamp 后应保持 5"
    );
}

/// 旧配置缺少两个 autosave 字段时不得反序列化失败。
#[test]
fn test_legacy_config_without_autosave_deserializes_to_defaults() {
    let mut value = serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
    let object = value
        .as_object_mut()
        .expect("UiConfig 序列化结果应是 JSON 对象");
    assert!(
        object.remove("autosave_enabled").is_some(),
        "默认配置应写出 autosave_enabled"
    );
    assert!(
        object.remove("autosave_interval_mins").is_some(),
        "默认配置应写出 autosave_interval_mins"
    );

    let restored: UiConfig =
        serde_json::from_value(value).expect("缺自动保存字段的旧配置应能反序列化");
    assert!(
        !restored.autosave_enabled,
        "旧配置缺开关字段应落为 false（关闭）"
    );
    assert_eq!(
        restored.autosave_interval_mins, 5,
        "旧配置缺间隔字段应落为默认值 5"
    );
}

/// 间隔收敛：0 与超上界值都收敛到 1..=120。
#[test]
fn test_autosave_interval_effective_clamps() {
    let zero = UiConfig {
        autosave_interval_mins: 0,
        ..UiConfig::default()
    };
    assert_eq!(
        zero.effective_autosave_interval_mins(),
        1,
        "0 分钟应收敛到下界 1"
    );

    let over = UiConfig {
        autosave_interval_mins: 999,
        ..UiConfig::default()
    };
    assert_eq!(
        over.effective_autosave_interval_mins(),
        120,
        "999 分钟应收敛到上界 120"
    );

    let valid = UiConfig {
        autosave_interval_mins: 30,
        ..UiConfig::default()
    };
    assert_eq!(
        valid.effective_autosave_interval_mins(),
        30,
        "合法值应原样通过"
    );
}
