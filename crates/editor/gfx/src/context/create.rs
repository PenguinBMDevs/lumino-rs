//! `Context::new` 与其私有 helper（REND-020 自 context.rs 拆分）。
//!
//! 拆分动机：`context.rs` 超过 400 行红线（红线 10「文件超 400 行必须拆分」）；
//! 构造路径与运行时操作（`ops.rs`）分文件，拆分本身行为不变。
//!
//! 包含：present mode / 帧延迟选择、临时 instance、共享 GPU 初始化、
//! [`Context::new`] 的 surface / 格式 / 交换链配置，以及 REND-020 的停帧守卫。

use std::sync::Arc;

use crate::ContextError;
use crate::context::{Context, Result, SHARED_GPU, SharedGpu, SharedGpuState, gpu_snapshot};
use crate::device_status;
use crate::epoch::GpuEpoch;

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
        // Android（2026-10-19 真机实测）：Adreno/Vulkan 上 Mailbox/Immediate 组合
        // 在 `surface_configure` 阶段失败，wgpu-core 先把 presentation `take()` 走
        // （`device/global.rs:2022`），失败后永久留空 → 此后每帧 `get_current_texture`
        // 恒报 `NotConfigured` → 黑屏。已验证可渲染的最小配置是 `AutoVsync`
        // （见 `crates/tools/android-probe`，同机同栈跑通）。
        #[cfg(target_os = "android")]
        {
            if modes.contains(&wgpu::PresentMode::AutoVsync) {
                return wgpu::PresentMode::AutoVsync;
            }
            if modes.contains(&wgpu::PresentMode::Fifo) {
                return wgpu::PresentMode::Fifo;
            }
            if modes.contains(&wgpu::PresentMode::Mailbox) {
                return wgpu::PresentMode::Mailbox;
            }
            return wgpu::PresentMode::Immediate;
        }

        #[cfg(not(target_os = "android"))]
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

/// 创建临时 wgpu 实例（共享 GPU 尚未初始化时使用）。
fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: super::instance_flags(),
        ..Default::default()
    })
}

/// 按统一口径初始化共享 GPU（初建路径；与重连 worker 共用 `build_device_queue`）。
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

    let (device, queue) = super::build_device_queue(&adapter).await?;

    Ok(SharedGpu {
        instance: instance.clone(),
        adapter,
        device,
        queue,
    })
}

impl Context {
    /// 异步创建图形上下文。
    ///
    /// 创建 surface、复用或初始化共享 GPU 资源（adapter/device/queue）、
    /// 选择全局 sRGB 的纹理格式并配置交换链。若共享 GPU 已初始化则复用其中的
    /// instance 与设备句柄，避免创建重复管线。
    ///
    /// # 参数
    /// * `surface_factory` — 渲染目标工厂（给定 instance 创建 `Surface`；
    ///   重连时用它整体替换 Surface，见 [`crate::context::SurfaceFactory`]）
    /// * `width` — 初始宽度（像素，至少为 1）
    /// * `height` — 初始高度（像素，至少为 1）
    ///
    /// # 返回值
    /// * `Ok(Context)` — 创建成功
    /// * `Err(ContextError)` — surface / adapter / format / device 任一步骤失败；
    ///   停帧窗口内返回 [`ContextError::DeviceLost`]（REND-020）
    pub async fn new(
        surface_factory: impl Fn(
            &wgpu::Instance,
        ) -> std::result::Result<
            wgpu::Surface<'static>,
            wgpu::CreateSurfaceError,
        > + Send
        + Sync
        + 'static,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        // REND-020（#166）：停帧窗口内禁止创建依赖死设备的上下文。
        // 守卫必须先于 surface 创建与 `surface.configure`：对已丢失设备 configure
        // 会等待 GPU idle（wgpu 的 `maintain(wait_indefinitely)`），可能永久阻塞；
        // 调用方（对话框等）按「等待重连」处理，不得重试循环。
        device_status::ensure_device_available()?;

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

        let surface =
            surface_factory(&instance).map_err(|e| ContextError::SurfaceCreation(e.to_string()))?;

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
        #[cfg(not(target_os = "android"))]
        let linear_view_format = match format {
            wgpu::TextureFormat::Bgra8UnormSrgb => Some(wgpu::TextureFormat::Bgra8Unorm),
            wgpu::TextureFormat::Rgba8UnormSrgb => Some(wgpu::TextureFormat::Rgba8Unorm),
            _ => None,
        };
        // Android（2026-10-19 真机实测）：Adreno 的 surface format 列表不含线性
        // view_format，申请它会让 `validate_surface_configuration` 失败并丢掉
        // presentation（同 `select_present_mode` 注释）；已验证可渲染的探针
        // （`crates/tools/android-probe`）也不申请，故 Android 下留空。
        #[cfg(target_os = "android")]
        let view_formats: Vec<wgpu::TextureFormat> = Vec::new();
        #[cfg(not(target_os = "android"))]
        let view_formats = linear_view_format.map(|f| vec![f]).unwrap_or_default();

        let surface_config = wgpu::SurfaceConfiguration {
            // `COPY_DST` 由 `surface_usage()` 统一给出（不可省，见该函数注释）。
            usage: crate::context::surface_usage(),
            format,
            width,
            height,
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats,
            // 收紧帧延迟以减少输入延迟（见 desired_maximum_frame_latency 说明）：
            // 该值直接决定 UI 能领先 GPU 多少帧，也就决定全量音符渲染在
            // GPU 队列里堆积的深度。Android 固定为驱动安全值 2（探针同值）。
            #[cfg(target_os = "android")]
            desired_maximum_frame_latency: 2,
            #[cfg(not(target_os = "android"))]
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
            surface_factory: Box::new(surface_factory),
        })
    }
}
