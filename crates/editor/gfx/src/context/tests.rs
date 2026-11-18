//! `context` 模块回归测试（与主文件分离，保持 `context.rs` 行数可控）。

use super::{matching_adapter_index, surface_usage};

/// 构造测试用适配器信息（构造多卡场景；字段与 `wgpu::AdapterInfo` 对齐）。
fn adapter_info(name: &str, backend: wgpu::Backend, vendor: u32, device: u32) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: name.to_string(),
        vendor,
        device,
        device_type: wgpu::DeviceType::DiscreteGpu,
        driver: String::new(),
        driver_info: String::new(),
        backend,
    }
}

/// 交换链纹理用途必须同时包含 `RENDER_ATTACHMENT` 与 `COPY_DST`。
///
/// 回归背景（2026-10-19 Android 真机）：为对齐探针把 usage 收窄成仅
/// `RENDER_ATTACHMENT` 后，`copy_offscreen_to_surface`（离屏纹理 → 交换链的
/// `copy_texture_to_texture`，见 `render_thread/thread/blit.rs`）被 wgpu 校验
/// 拒绝。用户可见现象是「UI 框架/菜单正常，**钢琴卷帘整块黑**」——因为音符区
/// 走离屏合成，而 UI 框架直接画在交换链上。
#[test]
fn surface_usage_keeps_copy_dst() {
    let usage = surface_usage();

    assert!(
        usage.contains(wgpu::TextureUsages::COPY_DST),
        "交换链纹理缺少 COPY_DST：钢琴卷帘/瀑布流的离屏合成拷贝会被 wgpu 校验拒绝（音符区黑屏）"
    );
    assert!(
        usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT),
        "交换链纹理必须可作为渲染目标，否则 UI 无法直接绘制到交换链"
    );
}

/// REND-021：重连必须选中与旧适配器相同的那张卡（iGPU + dGPU 机器上不得换绑）。
#[test]
fn matching_adapter_index_picks_same_card() {
    let igpu = adapter_info("Intel(R) UHD Graphics", wgpu::Backend::Dx12, 0x8086, 0x9a60);
    let dgpu = adapter_info(
        "NVIDIA GeForce RTX 4060",
        wgpu::Backend::Dx12,
        0x10de,
        0x2882,
    );
    let candidates = [igpu, dgpu.clone()];
    assert_eq!(
        matching_adapter_index(&candidates, &dgpu),
        Some(1),
        "重连必须命中旧适配器，而不是枚举顺序里的第一张卡"
    );
}

/// 只有一张卡且不是旧卡时不得瞎猜（返回 None → 由上层告警兜底）。
#[test]
fn matching_adapter_index_rejects_other_card() {
    let igpu = adapter_info("Intel(R) UHD Graphics", wgpu::Backend::Dx12, 0x8086, 0x9a60);
    let dgpu = adapter_info(
        "NVIDIA GeForce RTX 4060",
        wgpu::Backend::Dx12,
        0x10de,
        0x2882,
    );
    assert_eq!(matching_adapter_index(&[igpu], &dgpu), None);
}

/// 名称带驱动后缀变化时，按 (backend, vendor, device) 仍能认回同一型号硬件。
#[test]
fn matching_adapter_index_falls_back_to_identity_triple() {
    let previous = adapter_info("NVIDIA RTX 4060", wgpu::Backend::Dx12, 0x10de, 0x2882);
    let renamed = adapter_info(
        "NVIDIA GeForce RTX 4060 Laptop GPU",
        wgpu::Backend::Dx12,
        0x10de,
        0x2882,
    );
    assert_eq!(matching_adapter_index(&[renamed], &previous), Some(0));
}

/// 不同后端即不同适配器——即使名称/厂商/设备号相同也不得匹配。
#[test]
fn matching_adapter_index_requires_same_backend() {
    let previous = adapter_info("NVIDIA RTX 4060", wgpu::Backend::Dx12, 0x10de, 0x2882);
    let vulkan = adapter_info("NVIDIA RTX 4060", wgpu::Backend::Vulkan, 0x10de, 0x2882);
    assert_eq!(matching_adapter_index(&[vulkan], &previous), None);
}

/// 空枚举（设备丢失后枚举不到任何适配器）返回 None，由兜底路径负责告警。
#[test]
fn matching_adapter_index_empty_candidates() {
    let previous = adapter_info("NVIDIA RTX 4060", wgpu::Backend::Dx12, 0x10de, 0x2882);
    assert_eq!(matching_adapter_index(&[], &previous), None);
}
