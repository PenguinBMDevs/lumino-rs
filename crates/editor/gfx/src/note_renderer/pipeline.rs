//! 渲染/计算管线与 bind group layout 创建
//!
//! 与 `init.rs`（渲染器组装）分离，保持单文件 ≤ 400 行。

use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 创建渲染 bind group layout。
    pub(super) fn create_render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("note_render_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 视图状态（当前音轨 + 静音位图）：统一全量渲染切轨/静音零重传
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 全部音符实例数据（只读 storage）：render pass 用可见索引读取原数据
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 本 chunk 的全局基准（复用 cull uniform 槽位数据）：
                // VS 用 `chunk_start + 本地可见索引` 得全局索引 → 深度跨 chunk 连续。
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    /// 创建计算 bind group layout。
    pub(super) fn create_cull_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("note_cull_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    /// 创建渲染管线（含 pipeline layout）。
    ///
    /// `opaque`：片元恒不透明（洋葱皮/主音符层的 `onion_note*.wgsl` FS 恒输出
    /// `alpha = 1.0`）时置 `true`，颜色目标改用替换写入——省掉 blend ROP 的
    /// 读改写，且不抑制 early-Z（PREF-005）。预览层 `note.wgsl` 有 70% alpha
    /// 哨兵分支，必须保持混合，置 `false`。
    pub(super) fn create_render_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        render_bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        opaque: bool,
    ) -> wgpu::RenderPipeline {
        let builder = crate::pipeline::RenderPipelineBuilder::new(device, "note_pipeline", shader)
            .bind_group(render_bind_group_layout)
            .vertex_buffer(Self::visible_index_buffer_layout())
            .triangle_strip()
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ));
        let builder = if opaque {
            builder.opaque_target(format)
        } else {
            builder.alpha_blended_target(format)
        };
        builder.build()
    }

    /// 创建直绘管线（PREF-005）：源索引来自 `@builtin(instance_index)`，
    /// **无顶点缓冲**，不依赖 compute cull 产出的可见索引列表。
    ///
    /// 与 `create_render_pipeline` 的差异只有「顶点入口 / 顶点缓冲 / 混合」三点，
    /// 深度状态与 bind group layout 完全一致 ⇒ 深度语义与旧路径逐位相同。
    ///
    /// `alpha_blend` 选择混合模式（与 `create_render_pipeline` 的 `opaque` 同义）：
    /// - 洋葱皮直绘（`false`，不透明）：重叠片元靠 early-Z 拒绝，越远越早被拒；
    /// - **预览层直绘（`true`，alpha 混合）**：预览矩形必须与下方文档音符混合
    ///   （哨兵分支的 70% alpha），且重叠的多个预览矩形按**提交序**叠加
    ///   ⇒ 「后来者居上」（z-order 闪烁修复，2026-10）。
    pub(super) fn create_direct_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        render_bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        alpha_blend: bool,
    ) -> wgpu::RenderPipeline {
        let builder =
            crate::pipeline::RenderPipelineBuilder::new(device, "note_direct_pipeline", shader)
                .vertex_entry("vs_direct")
                .bind_group(render_bind_group_layout)
                // 无顶点缓冲：实例索引来自 @builtin(instance_index)
                .triangle_strip();
        let builder = if alpha_blend {
            builder.alpha_blended_target(format)
        } else {
            builder.opaque_target(format)
        };
        builder
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build()
    }

    /// 创建计算管线（含 pipeline layout）。
    pub(super) fn create_cull_pipeline(
        device: &wgpu::Device,
        cull_shader: &wgpu::ShaderModule,
        cull_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::ComputePipeline {
        crate::pipeline::ComputePipelineBuilder::new(device, "note_cull_pipeline", cull_shader)
            .bind_group(cull_bind_group_layout)
            .build()
    }
}
