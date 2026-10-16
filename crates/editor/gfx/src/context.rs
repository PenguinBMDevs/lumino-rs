use std::sync::{Arc, RwLock};
use thiserror::Error;

use crate::device_status;
use crate::epoch::GpuEpoch;

mod ops;

/// 根据可用的 present modes 选择最优的 present mode
///
/// macOS 上使用 Mailbox 优先策略（2026-04-24）：
/// Immediate 在 macOS 上会导致画面撕裂和 WindowServer 负载激增，
/// Mailbox 保留低延迟同时启用垂直同步。
///
/// 其他平台保持 Immediate 优先以降低输入延迟。
fn select_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    // macOS：Mailbox 优先，消除画面撕裂
    #[cfg(target_os = "macos")]
    {
        if modes.contains(&wgpu::PresentMode::Mailbox) {
            return wgpu::PresentMode::Mailbox;
        }
        if modes.contains(&wgpu::PresentMode::Fifo) {
            return wgpu::PresentMode::Fifo;
        }
        if modes.contains(&wgpu::PresentMode::AutoVsync) {
            return wgpu::PresentMode::AutoVsync;
        }
        if modes.contains(&wgpu::PresentMode::Immediate) {
            tracing::warn!("仅 Immediate 模式可用——将禁用垂直同步，macOS 上可能出现画面撕裂");
            return wgpu::PresentMode::Immediate;
        }
        wgpu::PresentMode::AutoVsync
    }

    // 非 macOS 平台：Immediate 优先，降低输入延迟
    #[cfg(not(target_os = "macos"))]
    {
        if modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else if modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else if modes.contains(&wgpu::PresentMode::AutoVsync) {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::Fifo
        }
    }
}

/// 交换链允许的「CPU 领先 GPU 帧数」。
///
/// 这是限制「UI 领先渲染多少帧」的唯一有效杠杆：UI 每 present 一帧就携带一次全量
/// 音符渲染，若允许 CPU 领先 2 帧，GPU 队列里就常驻 2~3 帧全量渲染，输入到光子的
/// 内容延迟被放大到 2~3 个 GPU 帧（黑乐谱滚动拖拽全程高延迟的直接来源）。
/// 收紧到 1 帧后，UI 与 GPU 基本同步推进，队列深度不再随音符量放大。
///
/// 注意：`wait_for_frame` 的完成信号是**提交序**而非 GPU 执行完成，不能用它做
/// 队列背压（渲染线程全程无阻塞式 `device.poll(Wait)`）；因此背压交给交换链。
///
/// 可用 `LUMINO_FRAME_LATENCY` 覆盖（1~3）做真机 A/B。
fn desired_maximum_frame_latency() -> u32 {
    std::env::var("LUMINO_FRAME_LATENCY")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(DEFAULT_FRAME_LATENCY, |v| v.clamp(1, 3))
}

/// 默认交换链帧延迟（低延迟优先）
const DEFAULT_FRAME_LATENCY: u32 = 1;

/// 进程级共享的 wgpu 资源
///
/// 多窗口场景下重复创建 Instance/Adapter/Device 是启动瓶颈之一，
/// 因此把这些重量级资源提升到进程级，仅每个窗口保留自己的 Surface。
pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// 进程级共享 GPU 状态（REND-019 PR-B）。
///
/// 重连时整体替换 `inner` 并递增 `epoch`（PR-C）——旧代次的一切跨帧资源
/// 依据 [`GpuEpoch`] 判废并走既有重建路径（E2/E3），禁止跨代次复用。
#[derive(Clone)]
pub struct SharedGpuState {
    /// 当前 GPU 代次（每次 device 重建单调递增）
    pub epoch: GpuEpoch,
    /// 共享 GPU 资源（instance / adapter / device / queue 结构不变）
    pub inner: Arc<SharedGpu>,
}

/// 唯一全局入口（REND-019 PR-B，G1）。
///
/// - device/queue 只能经 [`gpu_snapshot`] 或调用方传入的
///   `(GpuEpoch, Arc<SharedGpu>)` 获取；禁止新增长期存放 `Arc<SharedGpu>`
///   的 `OnceLock` / `static`（既有 SHARED_ENGINE / blit CACHE 属改造对象，非绕过对象）；
/// - 读锁拿快照后立即放锁——禁止持锁做任何 wgpu 调用 / `.await` / 阻塞收发（G3）。
static SHARED_GPU: RwLock<Option<std::result::Result<SharedGpuState, String>>> = RwLock::new(None);

/// 根据构建配置决定 wgpu 实例 flags
///
/// Debug 构建保留校验层便于排错；Release 构建关闭所有校验，
/// 避免 device 创建和运行时产生额外开销。
pub(crate) fn instance_flags() -> wgpu::InstanceFlags {
    #[cfg(debug_assertions)]
    {
        wgpu::InstanceFlags::DEBUG
            | wgpu::InstanceFlags::VALIDATION
            | wgpu::InstanceFlags::GPU_BASED_VALIDATION
    }

    #[cfg(not(debug_assertions))]
    {
        wgpu::InstanceFlags::empty()
    }
}

fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: instance_flags(),
        ..Default::default()
    })
}

/// 按统一口径为 adapter 构建 device/queue（初建与重连共用，避免特征/限制漂移）。
async fn build_device_queue(
    adapter: &wgpu::Adapter,
) -> std::result::Result<(wgpu::Device, wgpu::Queue), ContextError> {
    let adapter_features = adapter.features();
    let limits = adapter.limits();

    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: adapter_features & wgpu::Features::default(),
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
                max_buffer_size: limits.max_buffer_size,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .map_err(|e| ContextError::DeviceRequest(e.to_string()))
}

async fn init_shared_gpu(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'static>,
) -> std::result::Result<SharedGpu, ContextError> {
    // ⚠️ 启动 GPU 兼容性检测（`device_check`）用的是**另一套选卡**：
    // `required_backends()` + `PowerPreference::HighPerformance` + 无 surface。
    // 而这里是 `Backends::all()` + 环境变量/默认偏好 + surface 兼容性 ⇒
    // 两者可能选中不同适配器。故启动门控指纹缓存的语义只是「被检测的那张卡
    // 仍在枚举列表中」，**不保证**运行时用的就是它（详见
    // `runner::device_check_policy::cache_hit` 的「不变量边界」）。
    let adapter = wgpu::util::initialize_adapter_from_env_or_default(instance, Some(surface))
        .await
        .map_err(|e| ContextError::AdapterCreation(e.to_string()))?;

    let (device, queue) = build_device_queue(&adapter).await?;

    Ok(SharedGpu {
        instance: instance.clone(),
        adapter,
        device,
        queue,
    })
}

/// 图形上下文创建与帧获取阶段可能发生的错误
#[derive(Error, Debug)]
pub enum ContextError {
    /// 创建 `wgpu::Surface` 失败
    #[error("{}: {}", lumino_core::i18n::tr("创建 surface 失败"), .0)]
    SurfaceCreation(String),
    /// 创建 `wgpu::Adapter` 失败
    #[error("{}: {}", lumino_core::i18n::tr("创建 adapter 失败"), .0)]
    AdapterCreation(String),
    /// 未找到可用的 preferred format
    #[error("{}", lumino_core::i18n::tr("获取 preferred format 失败"))]
    PreferredFormatNotFound,
    /// 请求 `wgpu::Device` 失败
    #[error("{}: {}", lumino_core::i18n::tr("请求 device 失败"), .0)]
    DeviceRequest(String),
    /// GPU 共享资源尚未初始化（`gpu_snapshot` 在初始化完成前被调用）
    #[error("{}", lumino_core::i18n::tr("GPU 尚未初始化"))]
    GpuUninitialized,
    /// 获取交换链帧失败（含 `wgpu::SurfaceError`）
    #[error("{}: {}", lumino_core::i18n::tr("获取帧失败"), .0)]
    FrameAcquisition(#[from] wgpu::SurfaceError),
}

/// 图形上下文操作的通用结果类型
pub type Result<T> = std::result::Result<T, ContextError>;

/// 获取当前 GPU 代次快照（REND-019 PR-B，G1：唯一允许的读取方式）。
///
/// 读锁拿快照后立即放锁——禁止持锁做任何 wgpu 调用 / `.await` / 阻塞收发（G3）。
/// 返回的 `Arc<SharedGpu>` 只允许在「一次性渲染 / 上传动作」内使用（G2）；
/// 跨帧持有者必须连 [`GpuEpoch`] 一起存，且每帧使用前比对。
///
/// # 错误
/// - [`ContextError::GpuUninitialized`]：共享 GPU 尚未初始化
/// - [`ContextError::AdapterCreation`]：初始化曾失败（透传当时存储的错误串）
pub fn gpu_snapshot() -> Result<(GpuEpoch, Arc<SharedGpu>)> {
    let guard = SHARED_GPU
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match guard.as_ref() {
        Some(Ok(state)) => Ok((state.epoch, Arc::clone(&state.inner))),
        Some(Err(e)) => Err(ContextError::AdapterCreation(e.clone())),
        None => Err(ContextError::GpuUninitialized),
    }
}

/// REND-019 PR-C：重连 worker——从旧共享状态重建 adapter/device（在专用线程执行，
/// 禁止在 UI 线程同步等待；结果经 channel 交回 UI 主循环安装）。
///
/// 复用同一 `Instance`：各窗口 `Surface` 挂在其中——换 instance 需把全部窗口
/// surface 重建；同 instance 下由各窗口 `rebind_device` 重配即可恢复（E2）。
pub fn recreate_shared_gpu_worker() -> std::result::Result<(GpuEpoch, SharedGpu), String> {
    let (old_epoch, old) = gpu_snapshot().map_err(|e| e.to_string())?;
    let instance = old.instance.clone();

    // 适配器选择沿用初建口径（env 覆盖 + 默认偏好）；无 surface 做兼容性检查
    let adapter = futures::executor::block_on(wgpu::util::initialize_adapter_from_env_or_default(
        &instance, None,
    ))
    .map_err(|e| format!("重连初始化适配器失败: {e}"))?;

    let (device, queue) = futures::executor::block_on(build_device_queue(&adapter))
        .map_err(|e| format!("重连请求设备失败: {e}"))?;

    // 回调接线与初建口径一致（uncaptured + device-lost 置位/通知）
    device.on_uncaptured_error(Arc::new(|err| {
        tracing::error!("wgpu uncaptured error: {err}")
    }));
    device.set_device_lost_callback(device_status::on_device_lost);

    Ok((
        old_epoch.next(),
        SharedGpu {
            instance,
            adapter,
            device,
            queue,
        },
    ))
}

/// REND-019 PR-C：安装重连产物为新的共享 GPU 状态（epoch 单调递增）。
///
/// 只替换 `SHARED_GPU` 槽位；停帧标志由重连流水线在基础设施重建完成后撤销。
pub fn install_recreated_shared_gpu(
    epoch: GpuEpoch,
    gpu: SharedGpu,
) -> std::result::Result<(), String> {
    // 写锁内只做赋值（G3：无 wgpu 调用）
    let mut guard = SHARED_GPU
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(Ok(SharedGpuState {
        epoch,
        inner: Arc::new(gpu),
    }));
    tracing::info!("共享 GPU 已重建并安装：epoch = {}", epoch.0);
    Ok(())
}

/// GPU 渲染上下文，封装 surface / adapter / device / queue 等 wgpu 资源
pub struct Context {
    /// 渲染目标 surface（窗口或离屏）
    pub surface: wgpu::Surface<'static>,
    /// 图形适配器（GPU 设备句柄）
    pub adapter: wgpu::Adapter,
    /// 命令队列（提交命令缓冲）
    pub queue: wgpu::Queue,
    /// 逻辑 GPU 设备（创建管线与缓冲）
    pub device: wgpu::Device,
    /// 当前 surface 纹理格式（优先 sRGB）
    pub format: wgpu::TextureFormat,
    /// 当前交换链配置（REND-019 PR-A：`resize` 与 `handle_frame_error` 的
    /// Outdated 重配共用同一份，避免两处拼装 `SurfaceConfiguration` 漂移）
    surface_config: wgpu::SurfaceConfiguration,
    /// 本上下文所属 GPU 代次（REND-019 PR-B：跨帧资源比对基准；PR-C 重连后
    /// 旧上下文整体丢弃、由新代次重建。读取走 `Context::epoch()`）
    epoch: GpuEpoch,
}

impl Context {
    /// 异步创建图形上下文。
    ///
    /// 创建 surface、复用或初始化共享 GPU 资源（adapter/device/queue）、
    /// 选择全局 sRGB 的纹理格式并配置交换链。若共享 GPU 已初始化则复用其中的
    /// instance 与设备句柄，避免创建重复管线。
    ///
    /// # 参数
    /// * `target` — 渲染目标（窗口句柄 / surface target）
    /// * `width` — 初始宽度（像素，至少为 1）
    /// * `height` — 初始高度（像素，至少为 1）
    ///
    /// # 返回值
    /// * `Ok(Context)` — 创建成功
    /// * `Err(ContextError)` — surface / adapter / format / device 任一步骤失败
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let target = target.into();
        // 初始尺寸同样做上限裁剪，避免最大化时瞬时超限触发 device lost
        // 需在 adapter 确定后用其 max_texture_dimension_2d 二次裁剪，此处先保底 max(1)
        let width = width.max(1);
        let height = height.max(1);
        puffin::profile_function!();

        // 如果共享资源已初始化，复用其中的 instance；否则先创建一个临时 instance
        // 用来创建 surface，随后再用该 surface 初始化 adapter/device。
        // REND-019 PR-B：读取统一走快照（读锁拿数据即放锁，G3）。
        let instance = match gpu_snapshot() {
            Ok((_, gpu)) => gpu.instance.clone(),
            Err(_) => create_instance(),
        };

        let surface = instance
            .create_surface(target)
            .map_err(|e| ContextError::SurfaceCreation(e.to_string()))?;

        // 快照读（锁内只做 clone / 状态判断，立即放锁——G3：禁止持锁做 wgpu 调用）
        let existing = {
            let guard = SHARED_GPU
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.as_ref() {
                Some(Ok(state)) => Some((state.epoch, Arc::clone(&state.inner))),
                Some(Err(e)) => return Err(ContextError::AdapterCreation(e.clone())),
                None => None,
            }
        };

        let (epoch, shared) = match existing {
            Some(snapshot) => snapshot,
            None => {
                // 无锁初始化（init_shared_gpu 为 async + wgpu 调用，必须在锁外执行——G3）
                let gpu = init_shared_gpu(&instance, &surface).await?;
                // 注册 uncaptured error handler，避免 Metal 报错直接 abort（yinhe:84）
                // 睡眠唤醒/maximize 大纹理时 Surface::configure 可能触发 idle 等待失败
                gpu.device.on_uncaptured_error(Arc::new(|err| {
                    tracing::error!("wgpu uncaptured error: {err}")
                }));
                // REND-019（PR-A）：回调置位全局停帧标志（红线 C1：只做日志 +
                // 原子写，禁止 wgpu 调用 / 锁 / 阻塞发送）。渲染线程与 UI 侧
                // 依据该标志停止对死设备的盲重试；PR-C 将在此接线事件循环
                // 通知与重连状态机。
                gpu.device
                    .set_device_lost_callback(device_status::on_device_lost);
                let gpu = Arc::new(gpu);
                // 写锁安装（双检：并发初始化时沿用先到者、丢弃本实例——
                // 与旧 OnceLock::set 语义一致；锁内只做赋值，无 wgpu 调用——G3）
                let mut guard = SHARED_GPU
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                match guard.as_ref() {
                    Some(Ok(state)) => (state.epoch, Arc::clone(&state.inner)),
                    _ => {
                        let epoch = GpuEpoch::INITIAL;
                        *guard = Some(Ok(SharedGpuState {
                            epoch,
                            inner: Arc::clone(&gpu),
                        }));
                        (epoch, gpu)
                    }
                }
            }
        };

        // R2 测试注入（`gpu-reconnect-test` 特性；默认构建为空操作，REND-019 §三 R2）
        device_status::maybe_spawn_device_lost_injection();

        let capabilities = surface.get_capabilities(&shared.adapter);

        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(ContextError::PreferredFormatNotFound)?;

        // 选择最优 PresentMode（macOS 上 Mailbox 优先，其他平台 Immediate 优先）
        let present_mode = select_present_mode(&capabilities.present_modes);
        tracing::info!("Selected present_mode: {:?}", present_mode);

        // 视口物理尺寸上限裁剪（yinhe:181）：Retina 最大化可达 10k+，超 8192 硬上限会
        // 导致 Metal 丢设备黑屏。按 adapter 上限裁剪，与离屏纹理保持一致。
        let max_dim = shared.adapter.limits().max_texture_dimension_2d;
        let width = width.min(max_dim);
        let height = height.min(max_dim);

        // sRGB 需提供 linear view_format 供 shader 采样，否则 Metal 大纹理创建失败
        let linear_view_format = match format {
            wgpu::TextureFormat::Bgra8UnormSrgb => Some(wgpu::TextureFormat::Bgra8Unorm),
            wgpu::TextureFormat::Rgba8UnormSrgb => Some(wgpu::TextureFormat::Rgba8Unorm),
            _ => None,
        };
        let view_formats = linear_view_format.map(|f| vec![f]).unwrap_or_default();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format,
            width,
            height,
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats,
            // 收紧帧延迟以减少输入延迟（见 desired_maximum_frame_latency 说明）：
            // 该值直接决定 UI 能领先 GPU 多少帧，也就决定全量音符渲染在
            // GPU 队列里堆积的深度。
            desired_maximum_frame_latency: desired_maximum_frame_latency(),
        };
        surface.configure(&shared.device, &surface_config);

        Ok(Self {
            surface,
            adapter: shared.adapter.clone(),
            queue: shared.queue.clone(),
            device: shared.device.clone(),
            format,
            surface_config,
            epoch,
        })
    }
}
