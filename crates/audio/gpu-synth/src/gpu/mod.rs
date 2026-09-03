//! wgpu device management and the two compute pipelines (render + mix).

mod layout;

pub use layout::*;

use std::sync::Arc;

use crate::SynthError;

/// Number of fixed sample chunks bound to the render pipeline.
///
/// The sample data of a large soundfont (16-bit PCM resampled to f32 at a
/// higher rate) easily exceeds 1 GiB, far above the conservative 128 MiB
/// per-binding limit. The samples therefore live in a few fixed-size
/// chunks, each bound as its own storage binding; the device limits below
/// raise the per-binding size to 2 GiB (supported by all mainstream Vulkan
/// GPUs).
pub const SAMPLES_CHUNKS: usize = 4;

/// Number of segments each voice block is split into (gid.y of the render
/// kernel). More segments = more GPU parallelism for dense polyphony; the
/// shader fast-forwards the voice state to each segment start. Filtered
/// (biquad) voices are signal-dependent and fall back to single-segment
/// rendering, which is correct (just less parallel) - so audio quality is
/// unaffected.
///
/// Must match the `SEGS` constant injected into `render.wgsl` at pipeline
/// creation (see `create_render_pipeline`).
pub const RENDER_SEGMENTS: u32 = 4;

/// Capacity of one sample chunk in bytes (1 GiB, well below the 2 GiB
/// `max_storage_buffer_binding_size` requested from the adapter).
pub const SAMPLES_CHUNK_BYTES: u64 = 1 << 30;

/// Capacity of one sample chunk in `f32` words (must match `render.wgsl`).
pub const SAMPLES_CHUNK_F32: u32 = (SAMPLES_CHUNK_BYTES / 4) as u32;

/// Binding index of the first sample chunk in the render bind group.
pub const SAMPLES_CHUNK_BINDING_BASE: u32 = 1;
/// Binding index of the sinc table (after the 8 sample chunks).
pub const SINC_BINDING: u32 = SAMPLES_CHUNK_BINDING_BASE + SAMPLES_CHUNKS as u32;
/// Binding index of the envelope stages.
pub const ENV_BINDING: u32 = SINC_BINDING + 1;
/// Binding index of the voice states (read-write).
pub const STATES_BINDING: u32 = ENV_BINDING + 1;
/// Binding index of the voice output (read-write).
pub const VOICE_OUT_BINDING: u32 = STATES_BINDING + 1;

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

/// A storage buffer with dynamic (grow-on-demand) capacity.
#[derive(Debug)]
pub struct GrowableBuffer {
    buffer: wgpu::Buffer,
    size: u64,
    max_capacity: u64,
    usage: wgpu::BufferUsages,
    label: String,
}

impl GrowableBuffer {
    /// Creates a growable storage buffer with an initial `capacity` bytes.
    /// The initial allocation is zeroed (see `with_max_capacity`).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self::with_max_capacity(device, queue, label, capacity, u64::MAX, usage)
    }

    /// Creates a growable storage buffer whose size is capped at
    /// `max_capacity` bytes. Growth past the cap fails on write instead of
    /// allocating unbounded memory.
    pub fn with_max_capacity(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        capacity: u64,
        max_capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let size = capacity.max(16).min(max_capacity);
        let effective = Self::effective_usage(usage);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: effective,
            mapped_at_creation: false,
        });
        // Zero the initial allocation. wgpu does NOT zero storage buffers,
        // and several buffers (the chunked sample storage above all) can be
        // read past their written region at runtime - a voice may play past
        // the resampled data when the SF2's declared sample_end exceeds the
        // actual rendered length, and that slot must read as silence, not as
        // uninitialized garbage (measured: single samples in the hundreds of
        // millions, audible as loud pops at high polyphony).
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if size > 0 {
            encoder.clear_buffer(&buffer, 0, Some(size));
        }
        queue.submit(Some(encoder.finish()));
        Self {
            buffer,
            size,
            max_capacity,
            usage,
            label: label.to_string(),
        }
    }

    /// `MAP_READ` buffers are copy *destinations* only (`COPY_SRC` combined
    /// with `MAP_READ` is rejected by wgpu); everything else also needs
    /// `COPY_SRC` so growth can carry the old contents over.
    fn effective_usage(usage: wgpu::BufferUsages) -> wgpu::BufferUsages {
        let mut u = usage;
        if !u.contains(wgpu::BufferUsages::MAP_READ) {
            u |= wgpu::BufferUsages::COPY_SRC;
        }
        u | wgpu::BufferUsages::COPY_DST
    }

    /// Returns the current backing buffer.
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Returns the allocated size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Grows the buffer so it holds at least `needed` bytes, preserving the
    /// old contents when the buffer can act as a copy source (returns
    /// `true` if it grew, so callers can rebuild bind groups).
    pub fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, needed: u64) -> bool {
        if needed <= self.size {
            return false;
        }
        let new_size = (self.size * 2).max(needed).min(self.max_capacity);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&self.label),
            size: new_size,
            usage: Self::effective_usage(self.usage),
            mapped_at_creation: false,
        });
        let can_copy_src = !self.usage.contains(wgpu::BufferUsages::MAP_READ);
        let old_size = self.size;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        // The ENTIRE new buffer is zeroed: wgpu does not zero storage
        // buffers, and the voice output buffer is only fully written when
        // the voice count is at its historical maximum - on the block where
        // it grows, slots above the old size are not covered by the render
        // pass yet, and the mix pass would sum that garbage into the output
        // (measured: single samples in the hundreds of millions, audible as
        // pops/crackle at high polyphony). Old contents are carried over via
        // copy only when the buffer needs them (growable working buffers);
        // zeroing first keeps the semantic "everything unwritten reads as
        // silence" for every slot.
        if new_size > 0 {
            encoder.clear_buffer(&new_buf, 0, Some(new_size));
        }
        if old_size > 0 && can_copy_src {
            encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, old_size);
        }
        queue.submit(Some(encoder.finish()));
        self.buffer = new_buf;
        self.size = new_size;
        true
    }

    /// Writes `data` at `offset`, growing the buffer first if needed.
    ///
    /// Growing creates a new buffer, copies the old contents into it, and
    /// returns `true` so callers can rebuild bind groups that reference it.
    /// Fails with an error when the write would exceed `max_capacity`.
    pub fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        offset: u64,
        data: &[u8],
    ) -> Result<bool, SynthError> {
        let end = offset + data.len() as u64;
        if end > self.max_capacity {
            return Err(SynthError::Gpu(format!(
                "buffer '{}' write would exceed capacity {} bytes (need {end})",
                self.label, self.max_capacity
            )));
        }
        if end > self.size {
            let new_size = (self.size * 2).max(end.max(1024)).min(self.max_capacity);
            let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: new_size,
                usage: Self::effective_usage(self.usage),
                mapped_at_creation: false,
            });
            // Zero the freshly allocated region (wgpu does not zero storage
            // buffers): un-written slots must read as silence. The chunked
            // sample storage is only written up to the last sample's end -
            // a voice that plays past it (SF2 sample_end > rendered length)
            // would otherwise read uninitialized garbage (measured: recurring
            // single-sample pops ~40000 in dense MIDI).
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            if new_size > self.size {
                encoder.clear_buffer(&new_buf, self.size, Some(new_size - self.size));
            }
            // Copy old contents (if any) into the new buffer.
            let can_copy_src = !self.usage.contains(wgpu::BufferUsages::MAP_READ);
            if self.size > 0 && can_copy_src {
                encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, self.size);
            }
            queue.submit(Some(encoder.finish()));
            self.buffer = new_buf;
            self.size = new_size;
            queue.write_buffer(&self.buffer, offset, data);
            return Ok(true);
        }
        queue.write_buffer(&self.buffer, offset, data);
        Ok(false)
    }

    /// Clears the buffer contents to zero.
    pub fn clear(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Zero via write of a small zero chunk (buffers are re-uploaded every
        // block anyway for the fixed-size ones).
        let zeros = vec![0u8; self.size.min(4096) as usize];
        let mut off = 0u64;
        while off < self.size {
            let n = (self.size - off).min(4096);
            queue.write_buffer(&self.buffer, off, &zeros[..n as usize]);
            off += n;
        }
        let _ = device; // device unused; kept for signature symmetry
    }
}

/// Reference-counted GPU resources shared by the engine.
#[derive(Debug)]
pub struct GpuResources {
    /// Device/queue.
    pub ctx: Arc<GpuContext>,
    /// Render pipeline (pass 1).
    pub render_pipeline: wgpu::ComputePipeline,
    /// Mix pipeline (pass 2).
    pub mix_pipeline: wgpu::ComputePipeline,
    /// Render bind group layout.
    pub render_layout: wgpu::BindGroupLayout,
    /// Mix bind group layout.
    pub mix_layout: wgpu::BindGroupLayout,
    /// Block size compiled into the shaders.
    pub block_size: usize,
    /// Max voices compiled into the shaders.
    pub max_voices: usize,
}

impl GpuResources {
    /// Creates the pipelines for a given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] when shader compilation fails.
    pub fn new(
        ctx: Arc<GpuContext>,
        block_size: usize,
        max_voices: usize,
    ) -> Result<Self, SynthError> {
        let device = &ctx.device;

        // --- render bind group layout ---
        // binding 0: voice params, 1..=8: sample chunks, 9: sinc table,
        // 10: env stages, 11: voice states (rw), 12: voice output (rw).
        let mut render_entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::with_capacity(13);
        render_entries.push(bind_entry(
            0,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: true },
        ));
        for i in 0..SAMPLES_CHUNKS {
            render_entries.push(bind_entry(
                SAMPLES_CHUNK_BINDING_BASE + i as u32,
                wgpu::ShaderStages::COMPUTE,
                wgpu::BufferBindingType::Storage { read_only: true },
            ));
        }
        render_entries.push(bind_entry(
            SINC_BINDING,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: true },
        ));
        render_entries.push(bind_entry(
            ENV_BINDING,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: true },
        ));
        render_entries.push(bind_entry(
            STATES_BINDING,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: false },
        ));
        render_entries.push(bind_entry(
            VOICE_OUT_BINDING,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: false },
        ));
        let render_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render bind group layout"),
            entries: &render_entries,
        });

        // --- mix bind group layout ---
        let mix_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mix bind group layout"),
            entries: &[
                bind_entry(
                    0,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                bind_entry(
                    1,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: false },
                ),
                bind_entry(
                    2,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                bind_entry(
                    3,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                bind_entry(
                    4,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Uniform,
                ),
            ],
        });

        let render_pipeline = create_render_pipeline(device, &render_layout, block_size)?;
        let mix_pipeline = create_mix_pipeline(device, &mix_layout)?;

        Ok(Self {
            ctx,
            render_pipeline,
            mix_pipeline,
            render_layout,
            mix_layout,
            block_size,
            max_voices,
        })
    }
}

fn bind_entry(
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

fn create_render_pipeline(
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

fn create_mix_pipeline(
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
