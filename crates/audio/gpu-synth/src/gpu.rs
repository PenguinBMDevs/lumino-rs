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

/// REF-LGS-007：一个采样片段的容量，单位 **u32 字**（每个字装 2 帧 snorm16）。
///
/// 必须与 `render.wgsl` 的 `CHUNK_WORDS` 一致；按字寻址后同样 1 GiB 的片段
/// 可容纳两倍的采样帧。
pub const SAMPLES_CHUNK_WORDS: u32 = (SAMPLES_CHUNK_BYTES / 4) as u32;

/// REF-LGS-007：把 f32 采样打包成 2×snorm16（一个 u32 两帧），并返回归一化增益。
///
/// 低 16 位 = 偶数下标（WGSL `unpack2x16snorm(...).x`），高 16 位 = 奇数下标。
/// 量化按 snorm16 的满量程 `±32767`。
///
/// **为什么需要增益**：SF2 里存在峰值 >1.0 的采样（`rich.sf2` 的 sample 2 峰值
/// 1.67，4.2% 的帧越界；旧 f32 池不做限幅）。snorm16 只有 ±1，直接 clamp 会削波
/// （实测 −52 dBFS 失真）。因此按 `gain = max(1.0, |peak|)` 缩放到 ±1 再量化，
/// shader 侧乘回 `gain`——量化误差相对采样峰值仍是 1 LSB。
///
/// 非有限值（NaN/Inf）不参与峰值计算、打包为 0（静音，不产生爆音）；奇数长度的
/// 最后一帧高半字补 0。量化点固定在**上传时**：不引入 dither（dither 只在导出
/// 最终位深转换时加）。
pub fn pack_samples_2x16snorm(samples: &[f32]) -> (Vec<u32>, f32) {
    let max_abs = samples.iter().fold(0.0f32, |m, x| {
        if x.is_finite() {
            m.max(x.abs())
        } else {
            m
        }
    });
    let gain = max_abs.max(1.0);
    let inv = 1.0 / gain;
    let quantize = |x: f32| -> u16 {
        if !x.is_finite() {
            return 0;
        }
        let q = (x * inv * 32767.0).clamp(-32767.0, 32767.0).round() as i16;
        q as u16
    };
    let mut out = Vec::with_capacity(samples.len().div_ceil(2));
    for pair in samples.chunks(2) {
        let lo = u32::from(quantize(pair[0]));
        let hi = pair.get(1).map_or(0, |&s| u32::from(quantize(s)));
        out.push(lo | (hi << 16));
    }
    (out, gain)
}

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
    use super::{
        MIX_CHUNK_FRAMES, SAMPLES_CHUNK_BYTES, SAMPLES_CHUNK_WORDS, pack_samples_2x16snorm,
        render_chunk_size,
    };

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

    /// WGSL `unpack2x16snorm` 的本地等价实现（x = 低半字，y = 高半字）。
    fn unpack2x16snorm(word: u32) -> (f32, f32) {
        let lo = (word & 0xFFFF) as u16 as i16 as f32 / 32767.0;
        let hi = (word >> 16) as u16 as i16 as f32 / 32767.0;
        (lo.max(-1.0), hi.max(-1.0))
    }

    /// REF-LGS-007：低半字必须是**偶数下标**（与 shader 的 `i & 1` 选择一致）。
    #[test]
    fn pack_2x16snorm_packs_even_index_in_low_half() {
        let samples = [0.0f32, 1.0, -1.0, 0.5, -0.5];
        let (packed, gain) = pack_samples_2x16snorm(&samples);
        assert_eq!(gain, 1.0, "范围内的采样不需要归一化");
        assert_eq!(packed.len(), 3, "5 帧 → 3 个字（末帧补 0）");

        let (a, b) = unpack2x16snorm(packed[0]);
        assert_eq!(a, 0.0, "低半字 = 下标 0");
        assert!((b - 1.0).abs() < 1e-6, "高半字 = 下标 1");

        let lsb = 1.0 / 32767.0 + 1e-9;
        let (c, d) = unpack2x16snorm(packed[1]);
        assert!((c + 1.0).abs() < 1e-6, "−1.0 必须可逆");
        assert!((d - 0.5).abs() < lsb, "0.5 的量化误差应 ≤ 1 LSB：{d}");

        let (e, f) = unpack2x16snorm(packed[2]);
        assert!((e + 0.5).abs() < lsb, "−0.5 的量化误差应 ≤ 1 LSB：{e}");
        assert_eq!(f, 0.0, "奇数长度的末帧高半字补静音");
    }

    /// REF-LGS-007：峰值 >1 的采样必须按 `gain = peak` 归一化后打包，否则
    /// snorm16 会削波（`rich.sf2` sample 2 峰值 1.67，实测 −52 dBFS 失真）。
    #[test]
    fn pack_2x16snorm_normalizes_over_full_scale_samples() {
        let samples = [1.67f32, -1.67, 0.835, -0.835];
        let (packed, gain) = pack_samples_2x16snorm(&samples);
        assert!((gain - 1.67).abs() < 1e-6, "gain 必须等于峰值：{gain}");
        let (a, b) = unpack2x16snorm(packed[0]);
        assert!((a * gain - 1.67).abs() < 1e-4, "乘回 gain 后必须还原峰值");
        assert!((b * gain + 1.67).abs() < 1e-4);
        let (c, d) = unpack2x16snorm(packed[1]);
        assert!((c * gain - 0.835).abs() < 1e-4);
        assert!((d * gain + 0.835).abs() < 1e-4);
    }

    /// 非有限值：不参与峰值、打包为静音。
    #[test]
    fn pack_2x16snorm_silences_non_finite() {
        let (packed, gain) = pack_samples_2x16snorm(&[f32::NAN, f32::INFINITY]);
        assert_eq!(gain, 1.0);
        let (a, b) = unpack2x16snorm(packed[0]);
        assert_eq!(a, 0.0, "NaN 静音");
        assert_eq!(b, 0.0, "inf 静音");
    }

    /// 片段容量按字计：同一个 1 GiB 片段容纳两倍帧数。
    #[test]
    fn chunk_words_double_the_frame_capacity() {
        assert_eq!(SAMPLES_CHUNK_WORDS as u64 * 4, SAMPLES_CHUNK_BYTES);
        assert_eq!(u64::from(SAMPLES_CHUNK_WORDS) * 2, SAMPLES_CHUNK_BYTES / 2);
    }
}
