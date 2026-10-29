//! 洋葱皮后端配置（Step 3）序列化契约测试。
//!
//! 独立于 `config.rs`（该文件已接近 400 行上限）：这里只验证三件事——
//! 默认是 GPU、旧配置缺字段仍可反序列化为 GPU、取值可正确往返。

use super::*;

/// 默认后端必须是 GPU：Step 3 不改变任何现有启动/渲染行为。
#[test]
fn test_onion_skin_backend_defaults_to_gpu() {
    assert_eq!(
        UiConfig::default().onion_skin_backend,
        OnionSkinBackend::Gpu,
        "默认洋葱皮后端必须是 GPU（现有行为不变）"
    );
    assert_eq!(
        OnionSkinBackend::default(),
        OnionSkinBackend::Gpu,
        "枚举默认值必须与 UiConfig 默认值一致"
    );
}

/// 旧配置（Step 3 之前写入的 `config.json`）缺少 `onion_skin_backend`：
/// 反序列化不得失败，字段落为 `Gpu`。
#[test]
fn test_legacy_config_without_onion_backend_deserializes_to_gpu() {
    let mut value = serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
    let removed = value
        .as_object_mut()
        .expect("UiConfig 序列化结果应为 JSON 对象")
        .remove("onion_skin_backend");
    assert!(
        removed.is_some(),
        "默认配置应写出 onion_skin_backend 键（以便本用例验证缺字段行为）"
    );

    let restored: UiConfig =
        serde_json::from_value(value).expect("缺洋葱皮后端字段的旧配置应能反序列化");
    assert_eq!(
        restored.onion_skin_backend,
        OnionSkinBackend::Gpu,
        "旧配置缺字段应落为 Gpu"
    );
}

/// CPU 后端可正确往返，且落盘取值为 snake_case（`"cpu"` / `"gpu"`）。
#[test]
fn test_onion_skin_backend_serde_roundtrip() {
    let config = UiConfig {
        onion_skin_backend: OnionSkinBackend::Cpu,
        ..UiConfig::default()
    };
    let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
    assert!(
        json.contains("\"onion_skin_backend\":\"cpu\""),
        "后端必须落盘为 snake_case 取值：{json}"
    );
    let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
    assert_eq!(restored.onion_skin_backend, OnionSkinBackend::Cpu);

    // 显式写 "gpu" 也必须读回 Gpu（不做大小写/别名兼容，保持单一取值）
    let gpu: OnionSkinBackend =
        serde_json::from_str("\"gpu\"").expect("snake_case gpu 应能反序列化");
    assert_eq!(gpu, OnionSkinBackend::Gpu);
}

/// 展示文案（Step 4 设置页下拉用）：专有名词不随语言变化。
#[test]
fn test_onion_skin_backend_display() {
    assert_eq!(OnionSkinBackend::Gpu.to_string(), "GPU");
    assert_eq!(OnionSkinBackend::Cpu.to_string(), "CPU");
}
