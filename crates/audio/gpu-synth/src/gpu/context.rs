use super::*;

/// A ready-to-use GPU device/queue pair.
#[derive(Debug)]
pub struct GpuContext {
    /// The wgpu device.
    pub device: wgpu::Device,
    /// The wgpu queue.
    pub queue: wgpu::Queue,
    /// The adapter used (exposed for diagnostics).
    pub adapter_info: wgpu::AdapterInfo,
}

/// Creates a [`GpuContext`] using the default high-performance adapter.
///
/// Prefer Vulkan on desktop for its higher `maxStorageBuffersPerShaderStage`
/// (D3D12 is capped at 8, Vulkan exposes much more), but fall back to
/// Metal/DX12/GL on platforms where Vulkan is unavailable (macOS Metal,
/// Windows DX12, etc.). Using `all()` lets wgpu pick the best available
/// backend per platform while still requesting the high limits below —
/// adapters that cannot satisfy them will fail `request_device` and be
/// skipped by `request_adapter`.
///
/// # Errors
///
/// Returns [`SynthError::GpuInit`] when no usable adapter/device exists.
pub fn create_gpu_context() -> Result<GpuContext, SynthError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| SynthError::GpuInit(format!("request_adapter failed: {e:?}")))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lumino-gpu-synth"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits {
            // 4 sample chunks + sinc + env + states + voice_out + params.
            max_storage_buffers_per_shader_stage: 16,
            // 1 GiB chunks; Vulkan GPUs report 2 GiB - 1 storage buffer
            // range and buffer size.
            max_storage_buffer_binding_size: (1 << 31) - 1,
            max_buffer_size: (1 << 31) - 1,
            ..wgpu::Limits::default()
        },
        memory_hints: wgpu::MemoryHints::default(),
        experimental_features: Default::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| SynthError::GpuInit(format!("request_device failed: {e:?}")))?;

    let adapter_info = adapter.get_info();
    Ok(GpuContext {
        device,
        queue,
        adapter_info,
    })
}
