//! 钢琴卷帘网格渲染器
//!
//! 使用 GPU Fragment Shader 高效渲染无限网格，实现 O(1) 渲染时间。

use crate::gpu_resource_tracker::TrackedBuffer;

/// Camera Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridCameraUniform {
    pub viewport_size: [f32; 2],
    pub camera_pos: [f32; 2], // (scroll_x, scroll_y)
    pub zoom: [f32; 2],       // (zoom_x, zoom_y)
    pub margins: [f32; 2],    // (keyboard_width, ruler_height)
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_half_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
    pub ppq: f32,
    pub max_key_index: f32,
    pub canvas_offset: [f32; 2], // (offset_x, offset_y)
    /// 当前有效的拍号变化数量
    pub time_signature_count: u32,
    /// 对齐填充，保证 vec4 数组在 WGSL uniform 中满足 16 字节对齐
    pub _padding: [u32; 3],
    /// 拍号变化列表，每个 vec4 存储 (tick, 分子, 分母, 保留)
    pub time_signatures: [[u32; 4]; 16],
}

impl GridCameraUniform {
    /// 使用 Builder 模式构造，推荐方式。
    ///
    /// ```ignore
    /// GridCameraUniform::builder()
    ///     .viewport_size(1920.0, 1080.0)
    ///     .camera_pos(100.0, 50.0)
    ///     .zoom(1.0, 0.5)
    ///     .build()
    /// ```
    pub fn builder() -> GridCameraUniformBuilder {
        GridCameraUniformBuilder::default()
    }
}

/// [`GridCameraUniform`] 的 Builder。
///
/// 20 个字段均有默认值，只需设置需要变更的字段即可。
#[derive(Debug, Clone)]
pub struct GridCameraUniformBuilder {
    viewport_size: [f32; 2],
    camera_pos: [f32; 2],
    zoom: [f32; 2],
    margins: [f32; 2],
    color_bg: [f32; 4],
    color_bg_black_key: [f32; 4],
    color_bar: [f32; 4],
    color_beat: [f32; 4],
    color_half_beat: [f32; 4],
    color_grid: [f32; 4],
    color_key_line: [f32; 4],
    ppq: f32,
    max_key_index: f32,
    canvas_offset: [f32; 2],
    time_signatures: Vec<(u32, u8, u8)>,
}

impl Default for GridCameraUniformBuilder {
    fn default() -> Self {
        Self {
            viewport_size: [1.0, 1.0],
            camera_pos: [0.0, 0.0],
            zoom: [1.0, 1.0],
            margins: [0.0, 0.0],
            color_bg: [0.1, 0.1, 0.1, 1.0],
            color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
            color_bar: [0.3, 0.3, 0.3, 1.0],
            color_beat: [0.2, 0.2, 0.2, 1.0],
            color_half_beat: [0.15, 0.15, 0.15, 1.0],
            color_grid: [0.15, 0.15, 0.15, 1.0],
            color_key_line: [0.15, 0.15, 0.15, 1.0],
            ppq: 1920.0,
            max_key_index: 127.0,
            canvas_offset: [0.0, 0.0],
            time_signatures: vec![(0, 4, 4)],
        }
    }
}

impl GridCameraUniformBuilder {
    /// 设置视口尺寸
    pub fn viewport_size(mut self, width: f32, height: f32) -> Self {
        self.viewport_size = [width, height];
        self
    }

    /// 设置相机位置
    pub fn camera_pos(mut self, x: f32, y: f32) -> Self {
        self.camera_pos = [x, y];
        self
    }

    /// 设置缩放
    pub fn zoom(mut self, x: f32, y: f32) -> Self {
        self.zoom = [x, y];
        self
    }

    /// 设置边距（键盘宽度、标尺高度）
    pub fn margins(mut self, keyboard_width: f32, ruler_height: f32) -> Self {
        self.margins = [keyboard_width, ruler_height];
        self
    }

    /// 设置背景色
    pub fn color_bg(mut self, color: [f32; 4]) -> Self {
        self.color_bg = color;
        self
    }

    /// 设置黑键背景色
    pub fn color_bg_black_key(mut self, color: [f32; 4]) -> Self {
        self.color_bg_black_key = color;
        self
    }

    /// 设置小节线颜色
    pub fn color_bar(mut self, color: [f32; 4]) -> Self {
        self.color_bar = color;
        self
    }

    /// 设置拍线颜色
    pub fn color_beat(mut self, color: [f32; 4]) -> Self {
        self.color_beat = color;
        self
    }

    /// 设置半拍线颜色
    pub fn color_half_beat(mut self, color: [f32; 4]) -> Self {
        self.color_half_beat = color;
        self
    }

    /// 设置网格线颜色
    pub fn color_grid(mut self, color: [f32; 4]) -> Self {
        self.color_grid = color;
        self
    }

    /// 设置键位线颜色
    pub fn color_key_line(mut self, color: [f32; 4]) -> Self {
        self.color_key_line = color;
        self
    }

    /// 设置 PPQ
    pub fn ppq(mut self, ppq: f32) -> Self {
        self.ppq = ppq;
        self
    }

    /// 设置最大键索引
    pub fn max_key_index(mut self, max_key_index: f32) -> Self {
        self.max_key_index = max_key_index;
        self
    }

    /// 设置画布偏移
    pub fn canvas_offset(mut self, x: f32, y: f32) -> Self {
        self.canvas_offset = [x, y];
        self
    }

    /// 设置拍号变化列表
    pub fn time_signatures(mut self, time_signatures: Vec<(u32, u8, u8)>) -> Self {
        self.time_signatures = time_signatures;
        self
    }

    /// 构建 [`GridCameraUniform`]
    pub fn build(self) -> GridCameraUniform {
        let count = self.time_signatures.len().min(16) as u32;
        let mut ts_arr = [[0u32; 4]; 16];
        for (i, (tick, num, den)) in self.time_signatures.iter().take(16).enumerate() {
            ts_arr[i] = [*tick, *num as u32, *den as u32, 0];
        }
        GridCameraUniform {
            viewport_size: self.viewport_size,
            camera_pos: self.camera_pos,
            zoom: self.zoom,
            margins: self.margins,
            color_bg: self.color_bg,
            color_bg_black_key: self.color_bg_black_key,
            color_bar: self.color_bar,
            color_beat: self.color_beat,
            color_half_beat: self.color_half_beat,
            color_grid: self.color_grid,
            color_key_line: self.color_key_line,
            ppq: self.ppq,
            max_key_index: self.max_key_index,
            canvas_offset: self.canvas_offset,
            time_signature_count: count,
            _padding: [0; 3],
            time_signatures: ts_arr,
        }
    }
}

/// 网格线实例（已废弃，改用 GPU infinite grid 方案）。
///
/// 兼容旧代码的占位类型，保留以避免大面积联级修改。
#[deprecated(
    since = "0.2.0",
    note = "不再使用 CPU 实例生成网格线，GridRenderer 改用 GPU infinite grid 方案"
)]
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridLineInstance {
    /// 线段起点（屏幕坐标）
    pub start: [f32; 2],
    /// 线段终点（屏幕坐标）
    pub end: [f32; 2],
    /// 线段颜色 (RGBA)
    pub color: [f32; 4],
    /// 线段宽度（像素）
    pub width: f32,
    /// 对齐填充（保持 16 字节对齐）
    pub _padding: [f32; 3],
}

impl GridLineInstance {
    /// 创建一条网格线实例。
    ///
    /// # 参数
    /// * `start` — 线段起点（屏幕坐标）
    /// * `end` — 线段终点（屏幕坐标）
    /// * `color` — RGBA 颜色
    /// * `width` — 线宽（像素）
    pub fn new(start: [f32; 2], end: [f32; 2], color: [f32; 4], width: f32) -> Self {
        Self {
            start,
            end,
            color,
            width,
            _padding: [0.0; 3],
        }
    }
}

/// 网格渲染器准备参数（聚合 GridRenderer::prepare 的 18 个参数）
#[derive(Debug, Clone)]
pub struct GridPrepareParams {
    /// 视口尺寸 [width, height]
    pub viewport_size: (f32, f32),
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 垂直缩放（倍率）
    pub zoom_y: f32,
    /// 键盘分区宽度（屏宽，渲染起止均偏移量）
    pub keyboard_width: f32,
    /// 顶标尺高度（像素）
    pub ruler_height: f32,
    /// 画布背景色 (RGBA)
    pub color_bg: [f32; 4],
    /// 黑键区域背景色 (RGBA)
    pub color_bg_black_key: [f32; 4],
    /// 小节线颜色 (RGBA)
    pub color_bar: [f32; 4],
    /// 拍子线颜色 (RGBA)
    pub color_beat: [f32; 4],
    /// 半拍线颜色 (RGBA)
    pub color_half_beat: [f32; 4],
    /// 细分网格线颜色 (RGBA)
    pub color_grid: [f32; 4],
    /// 琴键分隔线颜色 (RGBA)
    pub color_key_line: [f32; 4],
    /// 分辨率（每四分音符 tick 数）
    pub ppq: f32,
    /// 最大琴键索引（决定网格的 keys 范围）
    pub max_key_index: f32,
    /// 画布水平偏移（像素）
    pub canvas_offset_x: f32,
    /// 画布垂直偏移（像素）
    pub canvas_offset_y: f32,
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
}

/// 网格渲染器
pub struct GridRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 视口 uniform 缓冲区
    camera_buffer: TrackedBuffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// 缓存的 uniform 数据（避免每帧重复构建）
    cached_uniform: Option<GridCameraUniform>,
}

impl GridRenderer {
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("shaders/infinite_grid.wgsl");

    /// 创建新的网格渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的网格渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader =
            crate::shader::create_shader_module(device, "infinite_grid_shader", Self::SHADER_SRC);

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("infinite_grid_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // 创建渲染管线，按 needs_depth 决定是否携带 depth-stencil 状态
        let pipeline =
            crate::pipeline::RenderPipelineBuilder::new(device, "infinite_grid_pipeline", &shader)
                .bind_group(&bind_group_layout)
                // 放弃 CPU 传递顶点（无 vertex buffer）
                .triangle_strip()
                .alpha_blended_target(format)
                .depth_stencil(
                    crate::constants::rendering::depth_stencil_state_read_only_for(needs_depth),
                )
                .build();

        let camera_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("infinite_grid_camera_uniform"),
                contents: bytemuck::cast_slice(&[GridCameraUniform::builder().build()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("infinite_grid_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.inner().as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            camera_buffer,
            bind_group,
            cached_uniform: None,
        }
    }
}

impl GridRenderer {
    /// 准备渲染数据（带缓存优化）
    pub fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        puffin::profile_function!();
        let viewport = GridCameraUniform::builder()
            .viewport_size(params.viewport_size.0, params.viewport_size.1)
            .camera_pos(params.scroll_x, params.scroll_y)
            .zoom(params.zoom_x, params.zoom_y)
            .margins(params.keyboard_width, params.ruler_height)
            .color_bg(params.color_bg)
            .color_bg_black_key(params.color_bg_black_key)
            .color_bar(params.color_bar)
            .color_beat(params.color_beat)
            .color_half_beat(params.color_half_beat)
            .color_grid(params.color_grid)
            .color_key_line(params.color_key_line)
            .ppq(params.ppq)
            .max_key_index(params.max_key_index)
            .canvas_offset(params.canvas_offset_x, params.canvas_offset_y)
            .time_signatures(params.time_signatures.clone())
            .build();

        if self.cached_uniform.as_ref() != Some(&viewport) {
            queue.write_buffer(
                self.camera_buffer.inner(),
                0,
                bytemuck::cast_slice(&[viewport]),
            );
            self.cached_uniform = Some(viewport);
        }
    }

    /// 绘制网格线
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 画一个全屏的四边形（4个顶点，使用 TriangleStrip）
        render_pass.draw(0..4, 0..1);
    }
}
