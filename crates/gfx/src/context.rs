use thiserror::Error;

/// 根据可用的 present modes 选择最优的 present mode
/// 优先级：Mailbox > Immediate > Fifo > AutoVsync
fn select_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else if modes.contains(&wgpu::PresentMode::Immediate) {
        wgpu::PresentMode::Immediate
    } else if modes.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        wgpu::PresentMode::AutoVsync
    }
}

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("创建 surface 失败: {0}")]
    SurfaceCreation(String),
    #[error("创建 adapter 失败: {0}")]
    AdapterCreation(String),
    #[error("获取 preferred format 失败")]
    PreferredFormatNotFound,
    #[error("请求 device 失败: {0}")]
    DeviceRequest(String),
    #[error("获取帧失败: {0}")]
    FrameAcquisition(#[from] wgpu::SurfaceError),
}

pub type Result<T> = std::result::Result<T, ContextError>;

pub struct Context {
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub queue: wgpu::Queue,
    pub device: wgpu::Device,
    pub format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
}

impl Context {
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(target)
            .map_err(|e| ContextError::SurfaceCreation(e.to_string()))?;

        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .await
            .map_err(|e| ContextError::AdapterCreation(e.to_string()))?;

        let adapter_features = adapter.features();

        let capabilities = surface.get_capabilities(&adapter);

        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(ContextError::PreferredFormatNotFound)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: adapter_features & wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|e| ContextError::DeviceRequest(e.to_string()))?;

        // 添加于2026-02-01，尝试解决音符不跟手的问题（2方案+1回退+1旧方案）
        let present_mode = select_present_mode(&capabilities.present_modes);
        tracing::info!("Selected present_mode: {:?}", present_mode);

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );

        Ok(Self {
            surface,
            adapter,
            queue,
            device,
            format,
            present_mode,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                width,
                height,
                present_mode: self.present_mode,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
    }

    pub fn with_frame(
        &self,
        f: impl FnOnce(&wgpu::SurfaceTexture, &wgpu::TextureView),
    ) -> Result<()> {
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
