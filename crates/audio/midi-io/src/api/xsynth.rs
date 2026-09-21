use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};

use crate::realtime::{ChannelMixHandle, RealtimeEventSender, RealtimeSynth};

mod api;
mod init;
mod runtime;
mod threads;

#[cfg(test)]
mod tests;

/// XSynth 运行时统计信息
#[derive(Debug, Clone, Copy, Default)]
pub struct XSynthStats {
    /// 当前活跃 voice 数量
    pub voice_count: u64,
    /// 渲染器平均负载 (0.0 - 1.0)
    pub average_renderer_load: f64,
    /// 缓冲区样本数
    pub buffer_samples: i64,
    /// 事件队列当前深度（跨通道取最后采样值，单位：事件数）
    ///
    /// 正常播放为个位数~几十；**持续上涨即说明接入侧跟不上**（引擎的接入上限 =
    /// `DRAIN_CAP × 2 × 块率 × 在用通道数`，实测 16 通道 ≈ 84 万事件/秒、
    /// 单通道仅 ≈ 5.1 万事件/秒）。详见
    /// `realtime/examples/ingest_ceiling.rs` 与 `docs/2026-09-19-XSynth实时后端性能扫描与修复记录.md`。
    pub event_queue_depth: i64,
    /// 事件队列深度高水位（自启动以来最大值，单位：事件数）
    pub event_queue_high_water: i64,
    /// 被丢弃的 NoteOn 总数（紧急模式直接丢弃 + 洪峰队列冲洗）
    ///
    /// 保命闸关闭且未进入紧急模式时恒为 0，即"无丢音"。
    pub emergency_dropped_notes: u64,
}

/// XSynth 后端打开选项
#[derive(Debug, Clone)]
pub struct XSynthOptions {
    /// 缓冲区时长（毫秒）
    pub buffer_ms: f64,
    /// 每个键的最大并发 voice 数（None / 0 = 不限制；上限 128）。
    /// 对应 `ChannelConfigEvent::SetLayerCount`，调高减少偷声但增加渲染负载。
    pub max_voices_per_key: Option<usize>,
    /// 采样率
    pub sample_rate: u32,
    /// 每通道活跃声部上限（None = 不限，Some(0) 同样视为不限）。
    ///
    /// 注意：上游默认按**每通道**治理；多数黑乐谱跨多通道，实际总预算会被
    /// 通道数放大。实时路径固定 `None`，统一使用 `global_max_voices`（跨通道全局上限）。
    pub max_voices_per_channel: Option<usize>,
    /// 跨通道全局声部上限（硬上限/量程；None = 自动，引擎默认 10000）。
    ///
    /// 由渲染管线统一调度：运行目标 = `voice_target_ratio × 硬上限`，
    /// 超限时向"声部最多的通道"下发抢占命令，保证活跃声部总数有上界，
    /// 且新音符不被丢弃。
    pub global_max_voices: Option<usize>,
    /// 复音软目标比例：运行目标 = 比例 × `global_max_voices`（负载反馈只会更低）。
    ///
    /// 默认 `1 - 1/e ≈ 0.632`，留出约 37% 暂态余量，避免过载后的无休止正反馈。
    pub voice_target_ratio: f64,
    /// 过载保命闸（软 NPS 闸）：仅在重度过载时临时限速，默认关闭。
    ///
    /// 关闭时引擎不存在任何 NoteOn 丢弃路径。
    pub soft_nps_gate: bool,
    /// 音频播放输出设备（CPAL 音频设备名；None = 使用系统默认输出设备）
    pub audio_output_device: Option<String>,
}

/// 线程池逃生口环境变量（**仅供 A/B 复测**，不对外暴露为设置项）。
const THREAD_POOL_ENV: &str = "XSYNTH_THREAD_POOL";

/// 渲染块时长（ms）：MIDI 事件按渲染块边界批量应用，块越小音符落点量化误差越小。
const RENDER_WINDOW_MS: f64 = 10.0;

/// 缓冲目标地板（ms）。
///
/// 用户的"缓冲区"设置被用作**总缓冲目标**（与渲染块解耦），但后端强制 ≥ 该值：
/// 渲染尖峰与系统调度抖动需要深缓冲兜底，低于此值在重载下会出现欠载/爆音。
///
/// 与 UI 的不一致必须显式告警而非静默抬升：UI 滑块量程 5–100ms、
/// 默认值 30ms（`default_synth_buffer`），因此**默认配置本身就低于地板**，
/// 用户拉到任何值（含默认）实际都按 100ms 运行。
const MIN_CUSHION_MS: f64 = 100.0;

/// 归一化每键最大同音数：`None` / `0` = 不限制；其余夹紧到 1..=128。
///
/// 注意绝不能把 0 直接传给 `SetLayerCount(Some(0))`：xsynth 会立即偷声，
/// 导致该键所有新音符无声（0 按"不限制"处理是产品约定）。
fn normalize_max_voices_per_key(value: Option<usize>) -> Option<usize> {
    match value {
        None | Some(0) => None,
        Some(v) => Some(v.clamp(1, 128)),
    }
}

/// XSynth 软件合成后端，基于 realtime 合成管线提供实时 MIDI 播放
pub struct XSynth {
    synth: RealtimeSynth,
    /// 共享事件发送器（全量重建时替换，所有已创建的输出连接自动跟随）
    sender_shared: Arc<Mutex<RealtimeEventSender>>,
    /// 混音参数共享句柄（重建稳定：外层 `Arc` 指针不变，重建时替换内层 `Vec`）。
    /// 所有已创建的 `XSynthOutputConn` 通过它设置每通道增益/声像，
    /// 与 `sender_shared` 同生命周期语义。
    mixer_shared: ChannelMixHandle,
    /// 主输出实时响度峰值共享句柄（重建稳定：`clone_master_peak` 的 `Arc` 不变，
    /// 重建时跟随新管线），供输出连接读取主输出电平。
    master_peak_shared: Arc<AtomicU32>,
    /// 音色库路径（重建管线时重用）
    soundfont_path: PathBuf,
    /// 打开选项（重建管线时重用）
    options: Option<XSynthOptions>,
    version: String,
}
