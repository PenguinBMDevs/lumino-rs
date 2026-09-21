//! MidiConsole GPU 渲染器 — GPU 上下文

use super::{CellGpu, MidiconsoleGpuContext, MidiconsoleRenderer};

impl MidiconsoleGpuContext {
    /// 创建 GPU 上下文（无可用适配器时返回 `None`）。
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = futures::executor::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .ok()?;
        let (device, queue) =
            futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("midiconsole_gpu_export"),
                required_features: adapter.features() & wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))
            .ok()?;
        let renderer = MidiconsoleRenderer::new(&device, &queue, width, height);
        Some(Self {
            device,
            queue,
            renderer,
        })
    }

    /// 渲染一帧并以 RGBA 字节返回。
    pub fn render_frame(&mut self, cells: &[CellGpu], tick: u32) -> Vec<u8> {
        self.renderer
            .render_to_rgba(&self.device, &self.queue, cells, tick)
    }
}
