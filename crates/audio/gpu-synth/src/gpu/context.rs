use super::*;
use std::sync::OnceLock;

/// 进程级 device-lost 钩子（REND-019 PR-D §1.5 L1）。
///
/// LGS 使用**独立** device（与编辑器 SHARED_GPU 零共享，红线 8），因此丢失
/// 通知也独立：设备丢失回调只做日志 + 钩子调用——钩子在 wgpu 回调线程执行，
/// 只允许置标志 / 非阻塞通知（禁止 wgpu 调用 / 锁 / 阻塞；同编辑器侧红线 C1）。
///
/// REND-019 #152：**仅实时 LGS 链路创建的上文转发本钩子**；离线导出上下文
/// （`create_gpu_context_offline`）只置本地丢失标志——避免导出自身 device 的
/// 丢失被误报成"实时 LGS 丢失"而触发实时引擎重建。
static DEVICE_LOST_HOOK: OnceLock<Box<dyn Fn(String, String) + Send + Sync>> = OnceLock::new();

/// 注册 LGS device-lost 钩子（仅允许注册一次；上层经依赖转发注册）。
pub fn set_device_lost_hook(hook: impl Fn(String, String) + Send + Sync + 'static) {
    let _ = DEVICE_LOST_HOOK.set(Box::new(hook));
}

/// A ready-to-use GPU device/queue pair.
#[derive(Debug)]
pub struct GpuContext {
    /// The wgpu device.
    pub device: wgpu::Device,
    /// The wgpu queue.
    pub queue: wgpu::Queue,
    /// The adapter used (exposed for diagnostics).
    pub adapter_info: wgpu::AdapterInfo,
    /// REND-019 #152：本 device 是否已丢失（回调置位）。
    /// 离线渲染循环（有界读回等待）据此快速收场。
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// REND-019 #152：离线导出上下文（定向注入/归因分辨用）。
    /// 仅 `gpu-reconnect-test` 特性构建下读取（定向注入消费）。
    #[cfg_attr(not(feature = "gpu-reconnect-test"), allow(dead_code))]
    pub(crate) offline: bool,
}

impl GpuContext {
    /// 本 device 是否已丢失（REND-019 #152）。
    #[must_use]
    pub fn is_lost(&self) -> bool {
        self.lost.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Creates a [`GpuContext`] using the default high-performance adapter.
///
/// Prefer Vulkan on desktop for its higher `maxStorageBuffersPerShaderStage`
/// (D3D12 is capped at 8, Vulkan exposes much more), but fall back to
/// Metal/DX12/GL on platforms where Vulkan is unavailable (macOS Metal,
/// Windows DX12, etc.). Using `all()` lets wgpu pick the best available
/// backend per platform while still requesting the high limits below —
/// adapters that cannot satisfy them will fail `request_device` and be
/// skipped by `request_adapter`.
///
/// # Errors
///
/// Returns [`SynthError::GpuInit`] when no usable adapter/device exists.
pub fn create_gpu_context() -> Result<GpuContext, SynthError> {
    create_gpu_context_impl(true)
}

/// 离线导出专用（REND-019 #152）：device-lost 只置本地标志、**不转发**
/// 进程级钩子——导出的独立 device 丢失不应触发实时 LGS 重建链路
/// （见 [`DEVICE_LOST_HOOK`] 注释）。
///
/// # Errors
///
/// 同 [`create_gpu_context`]。
pub fn create_gpu_context_offline() -> Result<GpuContext, SynthError> {
    create_gpu_context_impl(false)
}

fn create_gpu_context_impl(forward_lost_hook: bool) -> Result<GpuContext, SynthError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| SynthError::GpuInit(format!("request_adapter failed: {e:?}")))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lumino-gpu-synth"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits {
            // 4 sample chunks + sinc + env + states + voice_out + params.
            max_storage_buffers_per_shader_stage: 16,
            // 1 GiB chunks; Vulkan GPUs report 2 GiB - 1 storage buffer
            // range and buffer size.
            max_storage_buffer_binding_size: (1 << 31) - 1,
            max_buffer_size: (1 << 31) - 1,
            ..wgpu::Limits::default()
        },
        memory_hints: wgpu::MemoryHints::default(),
        experimental_features: Default::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| SynthError::GpuInit(format!("request_device failed: {e:?}")))?;

    // REND-019 PR-D §1.5 L1 + #152：丢失回调（回调线程只做原子置位 + 非阻塞
    // 钩子调用）。实时上下文转发进程级钩子（UI 主循环消费后发起 LGS 重建）；
    // 离线上下文仅置标志，由导出侧有界读回等待快速收场。
    let lost = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cb_lost = std::sync::Arc::clone(&lost);
    device.set_device_lost_callback(move |reason: wgpu::DeviceLostReason, message: String| {
        cb_lost.store(true, std::sync::atomic::Ordering::SeqCst);
        let reason = format!("{reason:?}");
        if forward_lost_hook {
            tracing::error!("[gpu-synth] device lost: {reason} — {message}");
            if let Some(hook) = DEVICE_LOST_HOOK.get() {
                hook(reason, message);
            }
        } else {
            tracing::error!("[gpu-synth] offline context device lost: {reason} — {message}");
        }
    });

    // R2/R5 测试注入武装（仅 `gpu-reconnect-test`；默认构建不编译）
    #[cfg(feature = "gpu-reconnect-test")]
    crate::gpu::arm_injection_if_requested();

    let adapter_info = adapter.get_info();
    Ok(GpuContext {
        device,
        queue,
        adapter_info,
        lost,
        offline: !forward_lost_hook,
    })
}

/// R1 式验证（REND-019 PR-D L1）：独立 device destroy → 单次 poll 同步送达
/// 钩子（与编辑器侧 R1 不变量同口径）。
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static CAPTURED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

    #[test]
    fn test_lgs_device_lost_hook_fires_on_destroy() {
        set_device_lost_hook(|reason, message| {
            if let Ok(mut v) = CAPTURED.lock() {
                v.push((reason, message));
            }
        });
        let ctx = create_gpu_context().expect("创建 GPU 上下文失败");
        ctx.device.destroy();
        let _ = ctx.device.poll(wgpu::PollType::Poll);
        let fired = CAPTURED.lock().map(|v| !v.is_empty()).unwrap_or(false);
        assert!(fired, "destroy 后单次 poll 应同步送达 device-lost 钩子");
    }

    /// REND-019 #152：离线上下文（不转发钩子）销毁后本地丢失标志必须置位，
    /// 供离线渲染循环有界等待快速收场。
    #[test]
    fn test_offline_context_sets_local_lost_flag_on_destroy() {
        let ctx = create_gpu_context_offline().expect("创建离线 GPU 上下文失败");
        assert!(!ctx.is_lost(), "初始不应处于丢失状态");
        ctx.device.destroy();
        let _ = ctx.device.poll(wgpu::PollType::Poll);
        assert!(ctx.is_lost(), "destroy 后本地丢失标志应置位");
    }
}
