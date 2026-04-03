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

/// 橡皮擦工具行为模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EraserBehavior {
    /// 默认模式：Shift+拖动框选删除，普通点击删除单个
    #[default]
    Default,
    /// 直接框选模式：拖动框选删除，Shift+点击删除单个
    DirectSelect,
}

impl std::fmt::Display for EraserBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EraserBehavior::Default => write!(f, "默认 (Shift+拖动框选)"),
            EraserBehavior::DirectSelect => write!(f, "直接框选 (无需Shift)"),
        }
    }
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
    /// 橡皮擦工具行为模式
    #[serde(default)]
    pub eraser_behavior: EraserBehavior,
    /// 程序字体名称（系统字体名称）
    #[serde(default)]
    pub program_font_name: String,
    /// 程序字体路径（自定义字体路径，优先于 program_font_name）
    #[serde(default)]
    pub program_font_path: String,
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
            eraser_behavior: EraserBehavior::default(),
            program_font_name: String::new(),
            program_font_path: String::new(),
        }
    }
}
