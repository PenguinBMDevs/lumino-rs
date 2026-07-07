//! GPU 音频合成器 v2 — CPU 只切时间片，region 查找 + voice 生命周期全进 GPU
//!
//! 对比 v1 删掉了整层 CPU voice 管理（Voice, EnvPhase, send_note_on/off,
//! HashMap preset cache, key_idx, real_voice_counts）。
//!
//! 双 pass：event_proc（处理 raw events → voice params）
//!       → render（voice params → audio samples）

mod renderer;
mod synth;
mod wgsl;

use bytemuck::{Pod, Zeroable};

// ── 常量 ─────────────────────────────────────────────
/// GPU 合成器编译期最大 voice 数。
/// 实际使用的 voice 数通过 uniform `mv` 传入 shader，可配置且不超过此上限。
const MAX_VOICES: u32 = 2048;
const WGS: u32 = 256;
/// GPU 渲染每 chunk 的样本数。越小越实时（~21ms at 48kHz），
/// 但 dispatch 开销比例越高。1024 是 CPU 抽事件和 GPU 渲染的平衡点。
pub(crate) const GPU_BLOCK_SAMPLES: u32 = 1024;
const MAX_EVENTS: usize = 16_000_000;

// ── GPU 数据结构 ────────────────────────────────────
/// 紧凑 RawEvent：tick_offset(4B) + data(4B 打包 kind|channel|key|vel)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct RawEvent {
    pub tick_offset: u32,
    /// 打包: kind[7:0] | channel[15:8] | key[23:16] | vel[31:24]
    pub data: u32,
}

impl RawEvent {
    pub(crate) fn new(tick_offset: u32, kind: u32, channel: u32, key: u32, vel: u32) -> Self {
        Self {
            tick_offset,
            data: kind | (channel << 8) | (key << 16) | (vel << 24),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuRegion {
    key_low: u32,
    key_high: u32,
    vel_low: u32,
    vel_high: u32,
    buf_offset: u32,
    buf_length: u32,
    /// 播放起始偏移（相对 sample 开头，已重采样）
    sample_offset: u32,
    loop_start: u32,
    loop_end: u32,
    loop_mode: u32,
    root_key: u32,
    tune: i32,
    volume: f32,
    pan: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuVoiceParams {
    position: f32,
    pitch_ratio: f32,
    volume: f32,
    pan: f32,
    sample_start: u32,
    sample_end: u32,
    loop_start: u32,
    loop_end: u32,
    enabled: u32,
    is_looping: u32,
    channel: u32,
    key: u32,
    released: u32,
    release_frame: u32,
    /// 本块内音符触发开始的 sample offset（render shader 据此跳过 sidx < start_frame 的样本）
    start_frame: u32,
    /// Release 开始后累计渲染的 sample 数（跨块累加，保证 envelope 连续不重启）
    release_elapsed: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uni {
    ne: u32,
    nr: u32,
    ns: u32,
    sr: u32,
    /// 实际启用的最大 voice 数（<= MAX_VOICES）
    mv: u32,
    /// 输出通道数（1 = mono, 2 = stereo）
    ch: u32,
}

// ── Re-exports ───────────────────────────────────────
pub(crate) use renderer::{GpuRenderer, PendingRender};
pub(crate) use synth::GpuSynth;
pub(crate) use wgsl::{EVENT_PROC_SRC, RENDER_SRC};
