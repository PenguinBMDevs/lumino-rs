//! GPU buffer layouts (bytemuck POD structs matching the WGSL kernels).

use bytemuck::{Pod, Zeroable};

/// REF-LGS-010: 声部**静态**参数（64 B）——声部生命周期内不变，只在槽位分配时
/// 上传一次，之后每块不再重传。
///
/// 槽位（slot）由引擎分配并回收，声部列表压缩（`retain`）不会改变槽位，
/// 所以静态表不需要按块重排。每块上传的只有 [`VoiceParamsDyn`]。
///
/// 全部字段都是 4 字节标量，与 WGSL 默认 storage 布局一致、无 padding。
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VoiceParamsStatic {
    pub offset: u32,
    pub sample_end: u32,
    pub loop_mode: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub amp: f32,
    pub pan_l: f32,
    pub pan_r: f32,
    pub filter_on: u32,
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
    pub interp: u32,
    pub channels: u32,
}

impl VoiceParamsStatic {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// REF-LGS-010: 声部**动态**参数（44 B）——每块上传，按声部下标（紧凑顺序）
/// 索引；`slot` 指向 [`VoiceParamsStatic`]。
///
/// 这些字段都会在声部生命周期内变化：`speed` 受弯音/调音影响，
/// `sample_offset*` 在采样上传后才解析出真实偏移，
/// `env_*` 随包络阶段切换/淡出改写，`release_at` 随 NoteOff 变化。
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VoiceParamsDyn {
    /// 静态参数槽位（`u32::MAX` = 未分配）。
    pub slot: u32,
    pub is_active: u32,
    /// 释放起点（绝对帧；`u32::MAX` = 无释放）。
    pub release_at: u32,
    /// 包络模板在 `env_stages` 中的起始下标（REF-LGS-011）。
    pub env_base: u32,
    pub env_count: u32,
    pub release_idx: u32,
    pub finished_idx: u32,
    pub sample_offset: u32,
    pub sample_offset_r: u32,
    pub start_at: u32,
    pub speed: f32,
}

impl VoiceParamsDyn {
    pub const SIZE: usize = std::mem::size_of::<Self>();
    pub const RELEASE_AT_NONE: u32 = u32::MAX;
    /// 尚未分配静态槽位。
    pub const NO_SLOT: u32 = u32::MAX;
}

/// Mirror of `VoiceState` in `render.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VoiceState {
    pub int_time: u32,
    pub frac: f32,
    pub env_stage: u32,
    pub env_t: u32,
    pub env_from: f32,
    /// 当前包络值（跨 block 持久化；块首帧不能回退到 `env_from` 这一阶段起点值，
    /// 否则每个块边界都会产生一帧包络误差 —— 频谱全频柱/爆音的根因）。
    pub env_value: f32,
    pub lx1: f32,
    pub lx2: f32,
    pub ly1: f32,
    pub ly2: f32,
    pub rx1: f32,
    pub rx2: f32,
    pub ry1: f32,
    pub ry2: f32,
    pub last_loop_pos: u32,
    pub is_released: u32,
    pub ended: u32,
}

impl VoiceState {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

impl Default for VoiceState {
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Mirror of `EnvStageGpu` in `render.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct EnvStageGpu {
    pub kind: u32,
    pub target_val: f32,
    pub duration: u32,
}

impl EnvStageGpu {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// One controller event inside a block, applied frame-exactly by the mix
/// kernel (16 bytes). The kernel replays every event with `frame <= f`
/// against a per-channel copy of the block-start lerp state, so controller
/// changes take effect at their exact sample regardless of the block size
/// and of how many events a block contains.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MixEvent {
    /// Frame relative to the block start.
    pub frame: u32,
    /// 全局 MIDI 通道（0-255，`port*16+ch`，REND-002 #87）。
    pub channel: u32,
    /// Controller number (7 = volume, 11 = expression, 10/8 = pan).
    pub cc: u32,
    /// Controller value normalized to 0..1.
    pub value: f32,
}

/// Block-start controller lerp state for one channel (48 bytes, uniform
/// array element with 16-byte stride).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MixStart {
    pub vol: f32,
    pub vol_step: f32,
    pub vol_end: f32,
    pub expr: f32,
    pub expr_step: f32,
    pub expr_end: f32,
    pub pan: f32,
    pub pan_step: f32,
    pub pan_end: f32,
    /// Padding to a 16-byte uniform array element.
    pub _pad: [f32; 3],
}

/// MIDI 通道空间上限（16 端口 × 16 通道，REND-002 #87）。
///
/// `MixParams.starts` 定长按此分配；实际参与 mix 的通道数由
/// `MixParams.channel_count`（= `SynthConfig::midi_channels`）决定，
/// 单端口时循环只走 16 个通道，成本与历史一致。
pub const MAX_MIDI_CHANNELS: usize = 256;

/// REF-LGS-002：每个 render/mix dispatch 的帧块参数（uniform，动态偏移）。
///
/// `render_frame_base` 是**全局**帧号（render 的 `start_at`/`release_at`
/// 比较用）；`mix_frame_base` 是**块内相对**帧号（`MixEvent.frame` 与
/// `out_storage` 索引用）。
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ChunkParams {
    pub render_frame_base: u32,
    pub mix_frame_base: u32,
    pub chunk_size: u32,
    pub _pad: u32,
}

impl ChunkParams {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// Mirror of `MixParams` in `mix.wgsl` (uniform buffer, 16-byte padded).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MixParams {
    pub voice_count: u32,
    pub block_size: u32,
    pub channel_count: u32,
    pub event_count: u32,
    /// `sample_rate * 0.01`: the 10 ms lerp window in samples.
    pub lerp_len: f32,
    pub _pad: [f32; 3],
    /// Per-channel block-start lerp states.
    pub starts: [MixStart; MAX_MIDI_CHANNELS],
}

impl MixParams {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

impl Default for MixParams {
    fn default() -> Self {
        Self::zeroed()
    }
}
