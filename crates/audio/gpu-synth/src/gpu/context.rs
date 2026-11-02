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
/// （见 `DEVICE_LOST_HOOK` 注释）。
///
/// # Errors
///
/// 同 [`create_gpu_context`]。
pub fn create_gpu_context_offline() -> Result<GpuContext, SynthError> {
    create_gpu_context_impl(false)
}

/// 合成器期望的单 binding 上限（2 GiB - 1）：1 GiB 采样块必须整块绑定
/// （见 [`SAMPLES_CHUNK_BYTES`] 与 `gpu.rs` 中 `SAMPLES_CHUNKS` 的注释）。
const DESIRED_STORAGE_BINDING_SIZE: u32 = (1 << 31) - 1;

/// 合成器期望的单 buffer 上限（2 GiB - 1），与 binding 上限同口径。
const DESIRED_BUFFER_SIZE: u64 = (1 << 31) - 1;

/// 合成器期望的每 stage storage buffer 数：
/// 4 sample chunks + sinc + env + states + voice_out + params。
const DESIRED_STORAGE_BUFFERS_PER_STAGE: u32 = 16;

/// 把「期望限制」收敛到 adapter 上报能力之内。
///
/// `wgpu::Adapter::request_device` 是硬约束：`required_limits` 中任何一项
/// **超过** adapter 上报值都会让整台设备创建失败
/// （`RequestDeviceError::LimitsExceeded`），没有降级余地。此前这里硬写
/// 2 GiB - 1，在只上报 128 MiB `max_storage_buffer_binding_size` 的适配器上
/// （CI 的 Linux lavapipe、Mesa llvmpipe、部分老集显）会让
/// `create_gpu_context` 直接失败——Linux CI 因此红了整整一串 run。
///
/// 收敛口径 `min(期望值, adapter 上报值)`：能力足够的适配器行为完全不变；
/// 能力不足时退化为其上限，并由 [`warn_on_capability_shortfall`] 在创建
/// 上下文时可见地告警，不做静默降级。
fn required_limits(adapter_limits: &wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_storage_buffers_per_shader_stage: DESIRED_STORAGE_BUFFERS_PER_STAGE
            .min(adapter_limits.max_storage_buffers_per_shader_stage),
        max_storage_buffer_binding_size: DESIRED_STORAGE_BINDING_SIZE
            .min(adapter_limits.max_storage_buffer_binding_size),
        max_buffer_size: DESIRED_BUFFER_SIZE.min(adapter_limits.max_buffer_size),
        ..wgpu::Limits::default()
    }
}

/// adapter 撑不起单个 1 GiB 采样块时，必须在创建上下文时就留下可见痕迹：
/// 否则症状推迟到首次分配采样块才爆（`buffer size ... exceeds limit`），
/// 现场归因成本高。
fn warn_on_capability_shortfall(adapter_limits: &wgpu::Limits, adapter_info: &wgpu::AdapterInfo) {
    let binding_limit = u64::from(adapter_limits.max_storage_buffer_binding_size);
    if binding_limit < SAMPLES_CHUNK_BYTES {
        tracing::warn!(
            adapter = %adapter_info.name,
            backend = ?adapter_info.backend,
            binding_limit_bytes = binding_limit,
            chunk_bytes = SAMPLES_CHUNK_BYTES,
            "adapter 的 max_storage_buffer_binding_size 小于单个采样块，采样数据无法整块绑定",
        );
    }
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

    let adapter_info = adapter.get_info();
    let adapter_limits = adapter.limits();
    warn_on_capability_shortfall(&adapter_limits, &adapter_info);

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lumino-gpu-synth"),
        required_features: wgpu::Features::empty(),
        required_limits: required_limits(&adapter_limits),
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

    /// CI 回归（`test (ubuntu-latest)` 连续红）：只上报 128 MiB storage binding
    /// 的适配器（lavapipe / llvmpipe）上，required_limits 必须收敛到 adapter
    /// 能力内——否则 `request_device` 整体 `LimitsExceeded`，上下文建不出来。
    #[test]
    fn test_required_limits_clamped_to_weak_adapter() {
        let weak = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 8,
            max_storage_buffer_binding_size: 128 << 20,
            max_buffer_size: 256 << 20,
            ..wgpu::Limits::default()
        };

        let requested = required_limits(&weak);

        assert_eq!(requested.max_storage_buffers_per_shader_stage, 8);
        assert_eq!(requested.max_storage_buffer_binding_size, 128 << 20);
        assert_eq!(requested.max_buffer_size, 256 << 20);
        // `check_limits` 就是 wgpu `request_device` 判 `LimitsExceeded` 的同一判据：
        // 断言它为真 = 这个 adapter 上设备一定建得出来（旧代码在此为假）。
        assert!(
            requested.check_limits(&weak),
            "required_limits 必须落在 adapter 能力内（wgpu check_limits 判据）：{requested:?} vs {weak:?}"
        );
    }

    /// 能力足够的适配器不得被降级（高配卡仍拿 2 GiB - 1 / 16 buffers）。
    #[test]
    fn test_required_limits_keep_desired_on_capable_adapter() {
        let capable = wgpu::Limits {
            max_storage_buffers_per_shader_stage: DESIRED_STORAGE_BUFFERS_PER_STAGE,
            max_storage_buffer_binding_size: u32::MAX,
            max_buffer_size: u64::MAX,
            ..wgpu::Limits::default()
        };

        let requested = required_limits(&capable);

        assert_eq!(
            requested.max_storage_buffers_per_shader_stage,
            DESIRED_STORAGE_BUFFERS_PER_STAGE
        );
        assert_eq!(
            requested.max_storage_buffer_binding_size,
            DESIRED_STORAGE_BINDING_SIZE
        );
        assert_eq!(requested.max_buffer_size, DESIRED_BUFFER_SIZE);
        assert!(
            requested.check_limits(&capable),
            "高配 adapter 上不得因收敛而越界：{requested:?} vs {capable:?}"
        );
    }

    /// 不变量：任意 adapter 能力下，required_limits 都不得超过 adapter 上报值。
    #[test]
    fn test_required_limits_never_exceed_adapter() {
        let cases = [
            (4_u32, 64_u32 << 20, 128_u64 << 20),
            (8, 128 << 20, 256 << 20),
            (16, 2_u32 << 30, 4_u64 << 30),
            (32, u32::MAX, u64::MAX),
        ];

        for (buffers, binding, buffer) in cases {
            let adapter = wgpu::Limits {
                max_storage_buffers_per_shader_stage: buffers,
                max_storage_buffer_binding_size: binding,
                max_buffer_size: buffer,
                ..wgpu::Limits::default()
            };

            let requested = required_limits(&adapter);

            assert!(
                requested.check_limits(&adapter),
                "任意 adapter 能力下 required_limits 都不得越界：{requested:?} vs {adapter:?}"
            );
            // 反向保护：不得因为「收敛」把所需能力整体压到最小（那会静默降配）。
            assert!(
                requested.max_buffer_size >= 64 << 20
                    && requested.max_storage_buffer_binding_size >= 64 << 20,
                "收敛后仍须保留可用容量：{requested:?}"
            );
        }
    }
}
