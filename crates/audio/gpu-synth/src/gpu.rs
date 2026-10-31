//! wgpu device management and the two compute pipelines (render + mix).

mod buffer;
mod context;
#[cfg(feature = "gpu-reconnect-test")]
mod inject;
mod layout;
mod pipelines;
mod resources;

pub use buffer::GrowableBuffer;
pub use context::{
    GpuContext, create_gpu_context, create_gpu_context_offline, set_device_lost_hook,
};
#[cfg(feature = "gpu-reconnect-test")]
pub(crate) use inject::{arm_injection_if_requested, take_injection_pending};
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

/// REF-LGS-002: maximum number of frames a single render/mix dispatch
/// processes. A block larger than this is rendered in frame chunks that
/// REUSE the same `voice_out` buffer, so its peak memory is
/// `chunk × voices × 8 B` instead of `block × voices × 8 B`.
///
/// Chunking does not change the audio: every voice still advances frame by
/// frame and every per-channel sum keeps the original voice order.
pub const MIX_CHUNK_FRAMES: usize = 2048;

/// Frames processed per render/mix dispatch for a block size.
///
/// Always a power of two that divides the (power-of-two) block size, so the
/// last chunk is never partial.
pub fn render_chunk_size(block_size: usize) -> usize {
    block_size.clamp(16, MIX_CHUNK_FRAMES)
}

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
/// Binding index of the per-chunk parameters (render pass).
pub const CHUNK_PARAMS_RENDER_BINDING: u32 = VOICE_OUT_BINDING + 1;
/// Binding index of the static voice-parameter table (render pass, REF-LGS-010).
pub const PARAMS_STATIC_BINDING: u32 = CHUNK_PARAMS_RENDER_BINDING + 1;
/// Binding index of the per-chunk parameters (mix/reduce passes).
pub const CHUNK_PARAMS_MIX_BINDING: u32 = 6;

#[cfg(test)]
mod tests {
    use super::{MIX_CHUNK_FRAMES, render_chunk_size};

    /// 002: 分块必须整除块长，且不超过上限；小块保持原样（512 档零行为变化）。
    #[test]
    fn render_chunk_size_divides_power_of_two_blocks() {
        for block in [16usize, 64, 256, 512, 1024, 2048, 4096, 8192] {
            let chunk = render_chunk_size(block);
            assert!(chunk <= MIX_CHUNK_FRAMES, "chunk {chunk} > cap");
            assert_eq!(block % chunk, 0, "chunk {chunk} 必须整除 block {block}");
        }
        assert_eq!(render_chunk_size(512), 512);
        assert_eq!(render_chunk_size(1024), 1024);
        assert_eq!(render_chunk_size(2048), MIX_CHUNK_FRAMES);
        assert_eq!(render_chunk_size(8192), MIX_CHUNK_FRAMES);
    }
}
