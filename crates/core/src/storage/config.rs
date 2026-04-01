use serde::{Deserialize, Serialize};

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ui: UiConfig,
}

/// 用户界面配置默认值
impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SynthBackend {
    #[default]
    XSynth,
    Kdmapi,
    System,
}

impl std::fmt::Display for SynthBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynthBackend::XSynth => write!(f, "XSynth (内置)"),
            SynthBackend::Kdmapi => write!(f, "KDMAPI"),
            SynthBackend::System => write!(f, "系统 MIDI"),
        }
    }
}

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub theme: String,
    /// 用户偏好的合成器后端（用户设置中选择的）
    #[serde(default = "default_synth_backend")]
    pub preferred_backend: SynthBackend,
    /// 音色库路径
    #[serde(default)]
    pub soundfont_path: String,
    /// 是否使用经典系统标题栏（默认使用自定义标题栏）
    #[serde(default)]
    pub use_native_titlebar: bool,
    /// XSynth 渲染缓冲区大小(毫秒)，影响延迟和性能
    #[serde(default = "default_synth_buffer")]
    pub xsynth_buffer_ms: f64,
    /// XSynth 采样率
    #[serde(default = "default_synth_sample_rate")]
    pub xsynth_sample_rate: u32,
    /// XSynth 多线程 (-1=无, 0=自动, >0=线程数)
    #[serde(default = "default_synth_threads")]
    pub xsynth_threads: i32,
    /// XSynth 释放音符时是否淡出(避免爆音)
    #[serde(default = "default_synth_fade_out")]
    pub xsynth_fade_out_killing: bool,
}

fn default_synth_backend() -> SynthBackend {
    SynthBackend::XSynth
}

fn default_synth_buffer() -> f64 {
    20.0
}
fn default_synth_sample_rate() -> u32 {
    44100
}
fn default_synth_threads() -> i32 {
    0
}
fn default_synth_fade_out() -> bool {
    true
}

/// 用户界面配置默认值
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
            preferred_backend: SynthBackend::XSynth,
            soundfont_path: String::new(),
            use_native_titlebar: false,
            xsynth_buffer_ms: default_synth_buffer(),
            xsynth_sample_rate: default_synth_sample_rate(),
            xsynth_threads: default_synth_threads(),
            xsynth_fade_out_killing: default_synth_fade_out(),
        }
    }
}
