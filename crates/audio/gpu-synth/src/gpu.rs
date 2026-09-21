//! wgpu device management and the two compute pipelines (render + mix).

mod buffer;
mod context;
mod layout;
mod pipelines;
mod resources;

pub use buffer::GrowableBuffer;
pub use context::{GpuContext, create_gpu_context};
pub use layout::*;
pub use resources::GpuResources;

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
