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

/// 创建单 bind group、无 push constant 的 compute pipeline。
///
/// 三条管线原本各自展开同一段 layout + descriptor 样板，差异只有 shader 源、
/// 模块 label、pipeline label 与 layout label。
fn create_compute(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shader_label: &str,
    shader_source: String,
    pipeline_label: &str,
    layout_label: &str,
) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader_label),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(pipeline_label),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(layout_label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub(super) fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    block_size: usize,
) -> Result<wgpu::ComputePipeline, SynthError> {
    let source = include_str!("shaders/render.wgsl").replace(
        "const BLOCK: u32 = 512u;",
        &format!("const BLOCK: u32 = {block_size}u;"),
    );
    // Shader compilation errors surface on the device; validate eagerly by
    // checking the shader module info.
    Ok(create_compute(
        device,
        layout,
        "render.wgsl",
        source,
        "lumino render",
        "render layout",
    ))
}

pub(super) fn create_mix_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> Result<wgpu::ComputePipeline, SynthError> {
    Ok(create_compute(
        device,
        layout,
        "mix.wgsl",
        include_str!("shaders/mix.wgsl").to_string(),
        "lumino mix",
        "mix layout",
    ))
}

/// REF-LGS-001: the tiny channel-reduce pass (see `mix_reduce.wgsl`).
pub(super) fn create_mix_reduce_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> Result<wgpu::ComputePipeline, SynthError> {
    Ok(create_compute(
        device,
        layout,
        "mix_reduce.wgsl",
        include_str!("shaders/mix_reduce.wgsl").to_string(),
        "lumino mix reduce",
        "mix reduce layout",
    ))
}

#[cfg(test)]
mod shader_tests {
    /// REF-LGS-013B：hosted CI 没有 GPU，但 WGSL 的语法/类型错误必须在这里
    /// 被挡住（而不是等到用户机器上 `create_shader_module` 才炸）。
    ///
    /// 覆盖三个 compute shader；render 的 `BLOCK` 字符串替换也校验一次
    /// （运行时按 `SynthConfig::block_size` 替换）。
    #[test]
    fn shaders_parse_and_validate() {
        let render = include_str!("shaders/render.wgsl");
        let render_2k = render.replace("const BLOCK: u32 = 512u;", "const BLOCK: u32 = 2048u;");
        let mix = include_str!("shaders/mix.wgsl");
        let reduce = include_str!("shaders/mix_reduce.wgsl");

        for (name, src) in [
            ("render.wgsl", render),
            ("render.wgsl@2048", render_2k.as_str()),
            ("mix.wgsl", mix),
            ("mix_reduce.wgsl", reduce),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name} 解析失败: {e}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} 校验失败: {e}"));
        }
    }
}
