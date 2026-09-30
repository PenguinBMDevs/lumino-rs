//! 测试共享 GPU 设备（全 crate 单设备）。
//!
//! 为什么共享：每个测试各自 `request_adapter` / `request_device` 会在并行测试时
//! 同时持有十几台设备；Windows D3D12 驱动在设备创建/销毁与提交并发下可能整体
//! 停顿——表现为多个测试同时卡死、测试进程异常退出（本机实测：180 个测试中
//! 只剩 79 个跑完，其余 worker 全部堵在 GPU 调用上）。共享单设备把「多设备并发」
//! 降为「单设备多提交」，由 wgpu 内部串行化，不改变任何单测语义。
//!
//! 协议：
//! - `shared_device()` / `shared_instance_device()` 首次调用创建，进程内复用；
//! - 需要独立 limits/feature 的探测（如 `device_check`）仍自行创建设备，不经此处；
//! - `LUMINO_GFX_TEST_FALLBACK=1` 强制回退适配器（Windows WARP / Linux llvmpipe），
//!   用于本地复现 CI 软渲染行为。

use std::sync::OnceLock;

/// 共享测试上下文：实例 + 设备 + 队列（首次创建，进程内复用）。
static SHARED: OnceLock<(wgpu::Instance, wgpu::Device, wgpu::Queue)> = OnceLock::new();

/// 获取共享测试设备（不含实例）。
pub(crate) fn shared_device() -> (wgpu::Device, wgpu::Queue) {
    let (_, device, queue) = shared_instance_device();
    (device, queue)
}

/// 获取共享测试上下文（实例 + 设备 + 队列）。
pub(crate) fn shared_instance_device() -> (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    SHARED.get_or_init(create).clone()
}

/// 创建共享测试设备（仅首次调用）。
fn create() -> (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    use futures::executor::block_on;
    let force_fallback = std::env::var_os("LUMINO_GFX_TEST_FALLBACK").is_some();
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: force_fallback,
        ..Default::default()
    }))
    .expect("需要适配器");
    if force_fallback {
        let info = adapter.get_info();
        eprintln!(
            "[test_gpu] 强制回退适配器：{:?} backend={:?} device_type={:?}",
            info.name, info.backend, info.device_type
        );
    }
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lumino_gfx_shared_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求设备失败");
    (instance, device, queue)
}
