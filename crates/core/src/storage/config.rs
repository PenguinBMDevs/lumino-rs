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

/// 自动滚动模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AutoScrollMode {
    /// 模式1：固定指示线到左侧，卷帘自动左移
    FixedIndicatorLeft,
    /// 模式2：指示线移动，到右侧翻页
    #[default]
    ScrollingIndicator,
    /// 关闭自动滚动
    Off,
}

/// 自动滚动配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AutoScrollConfig {
    /// 当前自动滚动模式
    pub mode: AutoScrollMode,
    /// 模式1：指示线固定位置（从左边缘算起，像素）
    pub fixed_indicator_position: u32,
    /// 模式2：翻页触发位置（从右边缘算起，像素）
    pub page_trigger_offset: u32,
    /// 模式2：翻页后指示线回到的位置（从左边缘算起，像素）
    pub page_return_position: u32,
}

impl Default for AutoScrollConfig {
    fn default() -> Self {
        Self {
            mode: AutoScrollMode::default(),
            fixed_indicator_position: 200,
            page_trigger_offset: 100,
            page_return_position: 200,
        }
    }
}

impl std::fmt::Display for AutoScrollMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutoScrollMode::FixedIndicatorLeft => write!(f, "固定指示线 (卷帘滚动)"),
            AutoScrollMode::ScrollingIndicator => write!(f, "滚动指示线 (自动翻页)"),
            AutoScrollMode::Off => write!(f, "关闭"),
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
    /// XSynth 每个键允许的最大同音数（None=不限，默认16）
    /// 调高可减少密集钢琴/快速重复音符/拖音过程中的 voice stealing
    #[serde(default = "default_max_voices_per_key")]
    pub xsynth_max_voices_per_key: Option<usize>,
    /// 橡皮擦工具行为模式
    #[serde(default)]
    pub eraser_behavior: EraserBehavior,
    /// 程序字体名称（系统字体名称）
    #[serde(default)]
    pub program_font_name: String,
    /// 程序字体路径（自定义字体路径，优先于 program_font_name）
    #[serde(default)]
    pub program_font_path: String,
    /// 自动滚动配置
    #[serde(default)]
    pub auto_scroll: AutoScrollConfig,
    /// 力度过滤阈值（力度 <= 此值的音符不播放，0=关闭过滤，最大127）
    #[serde(default = "default_velocity_filter_threshold")]
    pub velocity_filter_threshold: u8,
    /// 是否启用 HiDPI 图标渲染（关闭时使用1x获得零性能开销，开启时使用2x获得视网膜清晰度）
    #[serde(default = "default_true")]
    pub icon_hidpi: bool,
}

fn default_true() -> bool {
    true
}

fn default_synth_backend() -> SynthBackend {
    SynthBackend::XSynth
}

fn default_synth_buffer() -> f64 {
    100.0
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
fn default_max_voices_per_key() -> Option<usize> {
    Some(16)
}
fn default_velocity_filter_threshold() -> u8 {
    1
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
            xsynth_max_voices_per_key: default_max_voices_per_key(),
            eraser_behavior: EraserBehavior::default(),
            program_font_name: String::new(),
            program_font_path: String::new(),
            auto_scroll: AutoScrollConfig::default(),
            velocity_filter_threshold: default_velocity_filter_threshold(),
            icon_hidpi: true,
        }
    }
}
