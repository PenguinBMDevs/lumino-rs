use super::pipelines::{bind_entry, create_mix_pipeline, create_render_pipeline};
use super::*;

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
