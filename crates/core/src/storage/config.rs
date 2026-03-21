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
}

fn default_synth_backend() -> SynthBackend {
    SynthBackend::XSynth
}

/// 用户界面配置默认值
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
            preferred_backend: SynthBackend::XSynth,
            soundfont_path: String::new(),
            use_native_titlebar: false,
        }
    }
}
