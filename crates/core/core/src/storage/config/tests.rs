use super::*;

/// DRAW-038：已看过的帮助 tip 必须能落盘/读回；旧配置缺字段默认为空。
#[test]
fn test_seen_help_tips_roundtrip_and_default() {
    let mut config = UiConfig {
        seen_help_tips: vec!["curve".into(), "text".into()],
        ..UiConfig::default()
    };
    let json = serde_json::to_string(&config).expect("序列化");
    let restored: UiConfig = serde_json::from_str(&json).expect("反序列化");
    assert_eq!(restored.seen_help_tips, vec!["curve", "text"]);

    config.seen_help_tips.clear();
    let mut value = serde_json::to_value(&config).expect("序列化");
    value
        .as_object_mut()
        .expect("对象")
        .remove("seen_help_tips");
    let restored: UiConfig = serde_json::from_value(value).expect("旧配置应可读");
    assert!(restored.seen_help_tips.is_empty());
}

/// 旧配置（UI-010 之前写入的 `config.json`）缺少 `gpu_last_check_time`：
/// 反序列化不得失败，字段落为 `None`，由调用方按"缓存过期"处理。
#[test]
fn test_legacy_config_without_gpu_check_time_deserializes_to_none() {
    let mut value = serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
    let removed = value
        .as_object_mut()
        .expect("UiConfig 序列化结果应为 JSON 对象")
        .remove("gpu_last_check_time");
    assert!(removed.is_some(), "默认配置应写出 gpu_last_check_time 键");

    let restored: UiConfig =
        serde_json::from_value(value).expect("缺时间戳字段的旧配置应能反序列化");
    assert_eq!(
        restored.gpu_last_check_time, None,
        "旧配置缺时间戳字段应落为 None（按缓存过期处理）"
    );
}

/// 新写入的时间戳可正确往返，供启动 TTL 判定读取。
#[test]
fn test_gpu_check_time_serde_roundtrip() {
    let config = UiConfig {
        gpu_last_check_time: Some(1_700_000_000),
        ..UiConfig::default()
    };
    let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
    let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
    assert_eq!(restored.gpu_last_check_time, Some(1_700_000_000));
}

/// Domino 互粘开关默认关闭：全新配置不得为用不到的 Domino 编码付费。
#[test]
fn test_domino_clipboard_defaults_to_disabled() {
    assert!(
        !UiConfig::default().domino_clipboard_enabled,
        "Domino 剪贴板互粘开关默认应为关闭"
    );
}

/// 旧配置（UI-016 之前写入的 `config.json`）缺少 `domino_clipboard_enabled`：
/// 反序列化不得失败，字段落为 `false`（关闭），保证升级后不为 Domino 编码付费。
#[test]
fn test_legacy_config_without_domino_clipboard_deserializes_to_false() {
    let mut value = serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
    let removed = value
        .as_object_mut()
        .expect("UiConfig 序列化结果应为 JSON 对象")
        .remove("domino_clipboard_enabled");
    assert!(
        removed.is_some(),
        "默认配置应写出 domino_clipboard_enabled 键"
    );

    let restored: UiConfig =
        serde_json::from_value(value).expect("缺 Domino 开关字段的旧配置应能反序列化");
    assert!(
        !restored.domino_clipboard_enabled,
        "旧配置缺该字段应落为 false（关闭）"
    );
}

/// 开启状态可正确往返，供设置面板与持久化读取。
#[test]
fn test_domino_clipboard_serde_roundtrip() {
    let config = UiConfig {
        domino_clipboard_enabled: true,
        ..UiConfig::default()
    };
    let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
    let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
    assert!(restored.domino_clipboard_enabled);
}

/// 旧配置缺少 GPU 缓冲状态指示器字段时应回落默认
/// （总开关/GOV 段 = 开，LOAD 口径 = 3 秒平均），不得导致反序列化失败。
#[test]
fn test_legacy_config_without_gpu_status_defaults() {
    let mut value = serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
    let obj = value
        .as_object_mut()
        .expect("UiConfig 序列化结果应为 JSON 对象");
    for key in [
        "gpu_buffer_status_enabled",
        "gpu_governor_metrics_enabled",
        "gpu_load_display",
    ] {
        assert!(
            obj.remove(key).is_some(),
            "默认配置应写出 {key} 键（以便本用例验证缺字段行为）"
        );
    }

    let restored: UiConfig =
        serde_json::from_value(value).expect("缺 GPU 指示器字段的旧配置应能反序列化");
    assert!(restored.gpu_buffer_status_enabled, "缺字段应落为默认开启");
    assert!(
        restored.gpu_governor_metrics_enabled,
        "缺字段应落为默认开启"
    );
    assert_eq!(
        restored.gpu_load_display,
        GpuLoadDisplay::ShortAverage,
        "缺字段应落为 3 秒平均"
    );
}

/// LOAD 口径可正确往返（瞬时模式持久化）。
#[test]
fn test_gpu_load_display_serde_roundtrip() {
    let config = UiConfig {
        gpu_load_display: GpuLoadDisplay::Instant,
        gpu_buffer_status_enabled: false,
        ..UiConfig::default()
    };
    let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
    let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
    assert_eq!(restored.gpu_load_display, GpuLoadDisplay::Instant);
    assert!(!restored.gpu_buffer_status_enabled);
}
