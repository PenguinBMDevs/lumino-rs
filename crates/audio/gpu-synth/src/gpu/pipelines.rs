use super::*;

pub(super) fn bind_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// REF-LGS-002: uniform binding addressed with a dynamic offset (one slot
/// per render/mix frame chunk).
pub(super) fn bind_entry_dynamic_uniform(
    binding: u32,
    min_size: u64,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: wgpu::BufferSize::new(min_size),
        },
        count: None,
    }
}

pub(super) fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    block_size: usize,
) -> Result<wgpu::ComputePipeline, SynthError> {
    let source = include_str!("shaders/render.wgsl")
        .replace(
            "const BLOCK: u32 = 512u;",
            &format!("const BLOCK: u32 = {block_size}u;"),
        )
        .replace(
            "const SEGS: u32 = 16u;",
            &format!("const SEGS: u32 = {}u;", RENDER_SEGMENTS),
        );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.wgsl"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("lumino render"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render layout"),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    // Shader compilation errors surface on the device; validate eagerly by
    // checking the shader module info.
    Ok(pipeline)
}

pub(super) fn create_mix_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> Result<wgpu::ComputePipeline, SynthError> {
    let source = include_str!("shaders/mix.wgsl");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mix.wgsl"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("lumino mix"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mix layout"),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    Ok(pipeline)
}

/// REF-LGS-001: the tiny channel-reduce pass (see `mix_reduce.wgsl`).
pub(super) fn create_mix_reduce_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> Result<wgpu::ComputePipeline, SynthError> {
    let source = include_str!("shaders/mix_reduce.wgsl");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mix_reduce.wgsl"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("lumino mix reduce"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mix reduce layout"),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    Ok(pipeline)
}
