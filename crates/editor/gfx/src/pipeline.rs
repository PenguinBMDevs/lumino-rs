//! 渲染/计算管线构建器
//!
//! 消除各 renderer 中重复的 wgpu 样板代码（见 docs/Lumino重构方案/文档三 §1.1）：
//! - pipeline layout + render pipeline 创建（原 ~30 行/处 → ~10 行/处）
//! - compute pipeline 创建
//!
//! 各 renderer 只需声明差异点（label / vertex buffers / topology / depth），
//! 其余字段由构建器统一填充，避免复制粘贴产生的漂移。

/// 渲染管线构建器
///
/// 封装 `create_pipeline_layout` + `create_render_pipeline` 的固定样板：
/// - 顶点入口默认 `vs_main`，片元入口默认 `fs_main`
/// - multisample / multiview / cache 使用默认值
pub struct RenderPipelineBuilder<'a> {
    device: &'a wgpu::Device,
    label: &'a str,
    shader: &'a wgpu::ShaderModule,
    vertex_entry: &'a str,
    fragment_entry: Option<&'a str>,
    bind_group_layouts: Vec<&'a wgpu::BindGroupLayout>,
    vertex_buffers: Vec<wgpu::VertexBufferLayout<'a>>,
    targets: Vec<Option<wgpu::ColorTargetState>>,
    primitive: wgpu::PrimitiveState,
    depth_stencil: Option<wgpu::DepthStencilState>,
}

impl<'a> RenderPipelineBuilder<'a> {
    /// 创建构建器。`label` 同时用于 pipeline 与 pipeline layout（`{label}_layout`）。
    pub fn new(device: &'a wgpu::Device, label: &'a str, shader: &'a wgpu::ShaderModule) -> Self {
        Self {
            device,
            label,
            shader,
            vertex_entry: "vs_main",
            fragment_entry: Some("fs_main"),
            bind_group_layouts: Vec::new(),
            vertex_buffers: Vec::new(),
            targets: Vec::new(),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
        }
    }

    /// 添加 bind group layout（按声明顺序绑定）。
    pub fn bind_group(mut self, layout: &'a wgpu::BindGroupLayout) -> Self {
        self.bind_group_layouts.push(layout);
        self
    }

    /// 添加 vertex buffer layout（按声明顺序绑定）。
    pub fn vertex_buffer(mut self, layout: wgpu::VertexBufferLayout<'a>) -> Self {
        self.vertex_buffers.push(layout);
        self
    }

    /// 便捷方法：切换为 TriangleStrip 拓扑（2D 实例化渲染常用）。
    pub fn triangle_strip(mut self) -> Self {
        self.primitive.topology = wgpu::PrimitiveTopology::TriangleStrip;
        self
    }

    /// 添加颜色目标（默认混合：ALPHA_BLENDING，全通道写入）。
    pub fn alpha_blended_target(mut self, format: wgpu::TextureFormat) -> Self {
        self.targets.push(Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        }));
        self
    }

    /// 添加颜色目标（无混合，覆盖写入）。
    pub fn opaque_target(mut self, format: wgpu::TextureFormat) -> Self {
        self.targets.push(Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }));
        self
    }

    /// 添加自定义颜色目标（需要完全控制时的逃生舱）。
    pub fn color_target(mut self, target: wgpu::ColorTargetState) -> Self {
        self.targets.push(Some(target));
        self
    }

    /// 设置深度/模板状态（`None` = 无 depth attachment）。
    ///
    /// 可通过 `crate::constants::rendering::depth_stencil_state_for` 生成。
    pub fn depth_stencil(mut self, state: Option<wgpu::DepthStencilState>) -> Self {
        self.depth_stencil = state;
        self
    }

    /// 构建渲染管线（同时创建 pipeline layout）。
    pub fn build(self) -> wgpu::RenderPipeline {
        let layout_label = format!("{}_layout", self.label);
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(layout_label.as_str()),
                bind_group_layouts: &self.bind_group_layouts,
                push_constant_ranges: &[],
            });

        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(self.label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(self.vertex_entry),
                    buffers: &self.vertex_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: self.fragment_entry.map(|entry| wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(entry),
                    targets: &self.targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: self.primitive,
                depth_stencil: self.depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
    }
}

/// 计算管线构建器
///
/// 封装 `create_pipeline_layout` + `create_compute_pipeline` 的固定样板，
/// 消除 note_renderer / waterfall / miditrail 等处的重复创建代码。
pub struct ComputePipelineBuilder<'a> {
    device: &'a wgpu::Device,
    label: &'a str,
    shader: &'a wgpu::ShaderModule,
    entry_point: &'a str,
    bind_group_layouts: Vec<&'a wgpu::BindGroupLayout>,
}

impl<'a> ComputePipelineBuilder<'a> {
    /// 创建构建器。`label` 同时用于 pipeline 与 pipeline layout（`{label}_layout`）。
    pub fn new(device: &'a wgpu::Device, label: &'a str, shader: &'a wgpu::ShaderModule) -> Self {
        Self {
            device,
            label,
            shader,
            entry_point: "main",
            bind_group_layouts: Vec::new(),
        }
    }

    /// 添加 bind group layout（按声明顺序绑定）。
    pub fn bind_group(mut self, layout: &'a wgpu::BindGroupLayout) -> Self {
        self.bind_group_layouts.push(layout);
        self
    }

    /// 构建计算管线（同时创建 pipeline layout）。
    pub fn build(self) -> wgpu::ComputePipeline {
        let layout_label = format!("{}_layout", self.label);
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(layout_label.as_str()),
                bind_group_layouts: &self.bind_group_layouts,
                push_constant_ranges: &[],
            });

        self.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(self.label),
                layout: Some(&pipeline_layout),
                module: self.shader,
                entry_point: Some(self.entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
    }
}

/// 共享测试设备创建（与 miditrail_renderer/tests/basic.rs 相同模式）。
#[cfg(test)]
pub(crate) fn test_device() -> (wgpu::Device, wgpu::Queue) {
    use futures::executor::block_on;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("需要适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("pipeline_builder_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求设备失败");
    (device, queue)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小合法 WGSL（pipeline 创建时 wgpu 会校验 shader 有效性）
    const TEST_WGSL: &str = r#"
        @vertex
        fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
            return vec4(0.0, 0.0, 0.0, 1.0);
        }
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4(1.0, 0.0, 0.0, 1.0);
        }
        @compute
        @workgroup_size(1)
        fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            _ = id;
        }
    "#;

    fn test_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
        crate::shader::create_shader_module(device, "test_shader", TEST_WGSL)
    }

    #[test]
    fn test_render_pipeline_builder_creates_pipeline() {
        let (device, _queue) = test_device();
        let shader = test_shader(&device);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test_layout"),
            entries: &[],
        });

        // 验证带 bind group + 混合目标的管线可成功创建（无 panic = 通过）
        let _pipeline = RenderPipelineBuilder::new(&device, "test_render_pipeline", &shader)
            .bind_group(&layout)
            .alpha_blended_target(wgpu::TextureFormat::Rgba8Unorm)
            .build();
    }

    #[test]
    fn test_render_pipeline_builder_triangle_strip() {
        let (device, _queue) = test_device();
        let shader = test_shader(&device);

        // triangle_strip + opaque + 无 bind group 的管线可成功创建
        let _pipeline = RenderPipelineBuilder::new(&device, "test_strip_pipeline", &shader)
            .triangle_strip()
            .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
            .build();
    }

    #[test]
    fn test_compute_pipeline_builder_creates_pipeline() {
        let (device, _queue) = test_device();
        let shader = test_shader(&device);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test_compute_layout"),
            entries: &[],
        });

        // 验证计算管线可成功创建
        let _pipeline = ComputePipelineBuilder::new(&device, "test_compute_pipeline", &shader)
            .bind_group(&layout)
            .build();
    }
}
