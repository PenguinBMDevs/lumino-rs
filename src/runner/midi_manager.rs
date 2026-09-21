use lumino_core::storage::config::{AudioEngineKind, SynthBackend, UiConfig};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// MIDI API 类型别名
type MidiApi = Box<dyn lumino_midi_io::Api>;
/// MIDI 输出连接类型别名
type MidiOutput = Box<dyn lumino_midi_io::OutputConnection>;
/// MIDI 初始化结果类型别名
type MidiInitResult = Result<(MidiApi, MidiOutput), String>;

/// 后端初始化结果
struct BackendInitResult {
    /// API 实例（用于保持合成器存活）
    api: Option<Box<dyn lumino_midi_io::Api>>,
    /// MIDI 输出连接
    output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
    /// 实际使用的后端类型
    backend: SynthBackend,
}

/// XSynth 异步初始化结果
enum XSynthInitResult {
    Success {
        api: Box<dyn lumino_midi_io::Api>,
        output: Box<dyn lumino_midi_io::OutputConnection>,
    },
    Failed(String),
}

/// LGS (GPU) 异步初始化结果
enum LgsInitResult {
    Success {
        api: Box<dyn lumino_midi_io::Api>,
        output: Box<dyn lumino_midi_io::OutputConnection>,
    },
    Failed(String),
}

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期
pub struct MidiManager {
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    api: Option<Box<dyn lumino_midi_io::Api>>,
    /// 备用 API 实例（用于 create_additional_output 创建独立播放连接时保持存活）
    fallback_api: Option<Box<dyn lumino_midi_io::Api>>,
    /// MIDI 输出连接
    output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
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
        }
    }
}

// ── 子模块 ──────────────────────────────────────────────────────────────

mod audio_action;
mod lgs;
mod outputs;
mod recovery;
mod xsynth;

pub use audio_action::handle_audio_action;
