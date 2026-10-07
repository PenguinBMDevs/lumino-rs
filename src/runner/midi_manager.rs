use lumino_core::storage::config::{AudioEngineKind, SynthBackend, UiConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, channel};

/// MIDI API 类型别名（REND-002：基础 trait 冻结，能力经 `SynthControl` 扩展）
type MidiApi = Box<dyn lumino_midi_io::SynthControl>;
/// MIDI 输出连接类型别名（REND-002：能力经 `PlaybackOutput` 扩展）
type MidiOutput = Box<dyn lumino_midi_io::PlaybackOutput>;
/// MIDI 初始化结果类型别名
type MidiInitResult = Result<(MidiApi, MidiOutput), String>;

/// 后端初始化结果
struct BackendInitResult {
    /// API 实例（用于保持合成器存活）
    api: Option<MidiApi>,
    /// MIDI 输出连接
    output: Option<MidiOutput>,
    /// 实际使用的后端类型
    backend: SynthBackend,
}

/// XSynth 异步初始化结果
enum XSynthInitResult {
    Success { api: MidiApi, output: MidiOutput },
    Failed(String),
}

/// LGS (GPU) 异步初始化结果
enum LgsInitResult {
    Success { api: MidiApi, output: MidiOutput },
    Failed(String),
}

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期
pub struct MidiManager {
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    api: Option<MidiApi>,
    /// 备用 API 实例（用于 create_additional_output 创建独立播放连接时保持存活）
    fallback_api: Option<MidiApi>,
    /// MIDI 输出连接
    output: Option<MidiOutput>,
    /// 实际启用的合成器后端
    active_backend: SynthBackend,
    /// 是否需要重新初始化
    needs_reinit: bool,
    /// 配置中偏好的后端（用于异步初始化后知道应该切换到哪个后端）
    preferred_backend: SynthBackend,
    /// XSynth 异步初始化接收器
    xsynth_init_rx: Option<Receiver<XSynthInitResult>>,
    /// 是否正在异步初始化 XSynth
    is_xsynth_initializing: bool,
    /// XSynth 音色库路径（用于 create_additional_output 回退创建时重用）
    xsynth_soundfont_path: String,
    /// XSynth 缓冲区大小（毫秒）
    xsynth_buffer_ms: f64,
    /// XSynth 采样率
    xsynth_sample_rate: u32,
    /// XSynth 每个键最大同音数
    xsynth_max_voices_per_key: Option<usize>,
    /// XSynth 全局最大并发 voice 数（硬上限/量程；None = 自动）
    xsynth_global_voice_limit: Option<usize>,
    /// XSynth 复音软目标比例（运行目标 = 比例 × 硬上限）
    xsynth_voice_target_ratio: f64,
    /// XSynth 过载保命闸（软 NPS 闸，默认关闭）
    xsynth_soft_nps_gate: bool,
    /// LGS (GPU) 异步初始化接收器
    lgs_init_rx: Option<Receiver<LgsInitResult>>,
    /// 是否正在异步初始化 LGS (GPU)
    is_lgs_initializing: bool,
    /// LGS (GPU) 音色库路径（用于 create_additional_output 回退创建时重用）
    lgs_soundfont_path: String,
    /// LGS (GPU) 渲染采样率（Hz）
    lgs_sample_rate: u32,
    /// LGS (GPU) 每块渲染帧数
    lgs_block_size: usize,
    /// LGS (GPU) 每个 (通道, 键) 最大同音数
    lgs_max_voices_per_key: usize,
    /// LGS (GPU) 是否使用 64 点 sinc 高质量插值
    lgs_use_sinc: bool,
    /// 系统 MIDI (WinMM) 输出设备 ID（None = 第一个/默认）
    winmm_output_device_id: Option<u32>,
    /// REND-002：当前文档期望的最大 MIDI 端口（0 = 单端口/Midi）。
    ///
    /// 由文档装载入口写入；XSynth 就绪时应用/重建，未就绪（异步初始化或
    /// System 回退）时暂存，待 `check_async_init_complete` 完成后对齐。
    desired_midi_max_port: u8,
    /// REND-002：启动本轮 XSynth 异步初始化时注入的布局；完成回调据此判断
    /// 是否需要再对齐（初始化期间文档可能已切换），避免刚初始化完又白重建一次。
    spawned_midi_max_port: u8,
    /// DEBT-05 #122：后台布局重建在途状态（一次性 worker + 防抖 + 合并）。
    layout_apply: Option<layout::LayoutApplyInFlight>,
    /// DEBT-05 #122：worker 在防抖窗口结束时读取的"最新期望布局"。
    layout_desired: Arc<AtomicU8>,
}

impl Default for MidiManager {
    fn default() -> Self {
        Self {
            api: None,
            fallback_api: None,
            output: None,
            active_backend: SynthBackend::System,
            needs_reinit: false,
            preferred_backend: SynthBackend::System,
            xsynth_init_rx: None,
            is_xsynth_initializing: false,
            xsynth_soundfont_path: String::new(),
            xsynth_buffer_ms: 0.0,
            xsynth_sample_rate: 0,
            xsynth_max_voices_per_key: None,
            xsynth_global_voice_limit: None,
            xsynth_voice_target_ratio: 1.0 - 1.0 / std::f64::consts::E,
            xsynth_soft_nps_gate: false,
            lgs_init_rx: None,
            is_lgs_initializing: false,
            lgs_soundfont_path: String::new(),
            lgs_sample_rate: 0,
            lgs_block_size: 0,
            lgs_max_voices_per_key: 0,
            lgs_use_sinc: false,
            winmm_output_device_id: None,
            desired_midi_max_port: 0,
            spawned_midi_max_port: 0,
            layout_apply: None,
            layout_desired: Arc::new(AtomicU8::new(0)),
        }
    }
}

// ── 子模块 ──────────────────────────────────────────────────────────────

mod audio_action;
mod layout;
mod lgs;
mod outputs;
mod recovery;
mod xsynth;

pub use audio_action::handle_audio_action;
