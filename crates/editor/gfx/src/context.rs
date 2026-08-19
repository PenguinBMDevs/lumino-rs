use std::sync::{Arc, OnceLock};
use thiserror::Error;

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

static SHARED_GPU: OnceLock<std::result::Result<Arc<SharedGpu>, String>> = OnceLock::new();

/// 根据构建配置决定 wgpu 实例 flags
///
/// Debug 构建保留校验层便于排错；Release 构建关闭所有校验，
/// 避免 device 创建和运行时产生额外开销。
fn instance_flags() -> wgpu::InstanceFlags {
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

async fn init_shared_gpu(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'static>,
) -> std::result::Result<SharedGpu, ContextError> {
    let adapter = wgpu::util::initialize_adapter_from_env_or_default(instance, Some(surface))
        .await
        .map_err(|e| ContextError::AdapterCreation(e.to_string()))?;

    let adapter_features = adapter.features();
    let limits = adapter.limits();

    let (device, queue) = adapter
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
        .map_err(|e| ContextError::DeviceRequest(e.to_string()))?;

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
    #[error("创建 surface 失败: {0}")]
    SurfaceCreation(String),
    /// 创建 `wgpu::Adapter` 失败
    #[error("创建 adapter 失败: {0}")]
    AdapterCreation(String),
    /// 未找到可用的 preferred format
    #[error("获取 preferred format 失败")]
    PreferredFormatNotFound,
    /// 请求 `wgpu::Device` 失败
    #[error("请求 device 失败: {0}")]
    DeviceRequest(String),
    /// 获取交换链帧失败（含 `wgpu::SurfaceError`）
    #[error("获取帧失败: {0}")]
    FrameAcquisition(#[from] wgpu::SurfaceError),
}

/// 图形上下文操作的通用结果类型
pub type Result<T> = std::result::Result<T, ContextError>;

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
    /// 当前呈现模式（同步用，`resize` 时复用）
    present_mode: wgpu::PresentMode,
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
        let width = width.max(1);
        let height = height.max(1);
        puffin::profile_function!();

        // 如果共享资源已初始化，复用其中的 instance；否则先创建一个临时 instance
        // 用来创建 surface，随后再用该 surface 初始化 adapter/device。
        let instance = SHARED_GPU
            .get()
            .and_then(|res| res.as_ref().ok().map(|shared| shared.instance.clone()))
            .unwrap_or_else(create_instance);

        let surface = instance
            .create_surface(target)
            .map_err(|e| ContextError::SurfaceCreation(e.to_string()))?;

        let shared = match SHARED_GPU.get() {
            Some(Ok(shared)) => Arc::clone(shared),
            Some(Err(e)) => return Err(ContextError::AdapterCreation(e.clone())),
            None => {
                let gpu = init_shared_gpu(&instance, &surface).await?;
                let gpu = Arc::new(gpu);
                let _ = SHARED_GPU.set(Ok(Arc::clone(&gpu)));
                gpu
            }
        };

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

        surface.configure(
            &shared.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                format,
                width,
                height,
                present_mode,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                // 降低帧延迟以减少输入延迟，提高响应性
                // 对于高帧率应用，1帧延迟比2帧更好
                desired_maximum_frame_latency: 2,
            },
        );

        Ok(Self {
            surface,
            adapter: shared.adapter.clone(),
            queue: shared.queue.clone(),
            device: shared.device.clone(),
            format,
            present_mode,
        })
    }

    /// 同步创建图形上下文（在无法使用 async 的初始化路径中使用）
    ///
    /// 内部使用 `futures::executor::block_on` 驱动异步初始化流程，
    /// 避免在多个窗口管理器中重复书写 block_on 样板。
    pub fn new_blocking(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        futures::executor::block_on(Self::new(target, width, height))
    }

    /// 调整交换链尺寸并重新配置 surface。
    ///
    /// # 参数
    /// * `width` — 新宽度（像素）
    /// * `height` — 新高度（像素；任一为 0 时直接忽略本次调整）
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                width,
                height,
                present_mode: self.present_mode,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
    }

    /// 获取当前交换链帧并在其上执行一次绘制。
    ///
    /// 若获取帧失败（如 OutOfMemory）则返回对应错误而不调用回调。
    ///
    /// # 参数
    /// * `f` — 接收帧纹理与视图的回调，在其中提交绘制命令
    ///
    /// # 返回值
    /// * `Ok(())` — 回调执行成功
    /// * `Err(ContextError::FrameAcquisition)` — 获取帧失败
    pub fn with_frame(
        &self,
        f: impl FnOnce(&wgpu::SurfaceTexture, &wgpu::TextureView),
    ) -> Result<()> {
        puffin::profile_function!();

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::OutOfMemory) => {
                tracing::error!("Swapchain error: OutOfMemory. Rendering cannot continue.");
                return Err(ContextError::FrameAcquisition(
                    wgpu::SurfaceError::OutOfMemory,
                ));
            }
            Err(e) => return Err(ContextError::FrameAcquisition(e)),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        f(&frame, &view);

        frame.present();

        Ok(())
    }
}
