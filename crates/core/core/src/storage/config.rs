use crate::types::Language;
use serde::{Deserialize, Serialize};

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 用户界面配置
    pub ui: UiConfig,
}

// 音轨标签格式同 yinhe：{通道字母}{通道号+1:02}，音轨始终按原始序号排列。
// 通道字母 ch0=A, ch1=B, ..., ch15=P，通道号 1-16（零填充两位数）。

/// 添加音轨时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackAddBehavior {
    /// 自动跳转到被添加的新音轨
    #[default]
    AutoSwitch,
    /// 保持当前音轨位置不变
    StayCurrent,
}

impl std::fmt::Display for TrackAddBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackAddBehavior::AutoSwitch => write!(f, "自动跳转到新音轨"),
            TrackAddBehavior::StayCurrent => write!(f, "保持当前音轨"),
        }
    }
}

/// 用户界面配置默认值
impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
        }
    }
}

/// 合成器后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SynthBackend {
    /// 内置 XSynth 合成器（默认）
    #[default]
    XSynth,
    /// KDMAPI 合成器（调用系统 KDMAPI）
    Kdmapi,
    /// 系统 MIDI 合成器
    System,
}

/// 框选框显示模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionBoxMode {
    /// 弹簧动画模式：框选框边界有弹性动画效果
    Spring,
    /// 直接跟随模式：框选框直接跟随鼠标，无动画延迟
    #[default]
    Direct,
}

impl std::fmt::Display for SelectionBoxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionBoxMode::Spring => write!(f, "弹簧动画"),
            SelectionBoxMode::Direct => write!(f, "直接跟随"),
        }
    }
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

/// 音频引擎后端（Realtime vs Core）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioEngineKind {
    /// Realtime：xsynth-realtime 多线程 + BufferedRenderer（lumino 原有）
    #[default]
    Realtime,
    /// Core：xsynth-core ChannelGroup + AudioRing SPSC（yinhe 复刻，零锁回调）
    Core,
}

impl std::fmt::Display for AudioEngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioEngineKind::Realtime => write!(f, "Realtime (xsynth)"),
            AudioEngineKind::Core => write!(f, "Core (ring)"),
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    /// 主题名称（例如 "Light"）
    #[serde(default)]
    pub theme: String,
    /// 界面语言
    #[serde(default)]
    pub language: Language,
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
    /// 框选框显示模式
    #[serde(default)]
    pub selection_box_mode: SelectionBoxMode,
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
    /// XSynth 全局最大并发 voice 数
    /// 设置越低，渲染越快，但并发发音数越少。
    /// None = 使用 xsynth 默认值 (4096)
    #[serde(default)]
    pub xsynth_global_voice_limit: Option<usize>,
    /// 是否启用 HiDPI 图标渲染（关闭时使用1x获得零性能开销，开启时使用2x获得视网膜清晰度）
    #[serde(default = "default_true")]
    pub icon_hidpi: bool,
    /// 是否启用 256 键扩展钢琴卷帘（默认关闭）
    #[serde(default)]
    pub enable_256key: bool,
    /// 力度面板显示样式（默认曲线=折线图，false=柱状图）
    #[serde(default = "default_true")]
    pub velocity_curve_style: bool,
    /// 高精度洋葱皮贴图：是否启用
    #[serde(default = "default_true")]
    pub hires_onion_enabled: bool,
    /// 高精度洋葱皮贴图：每组小节数（1-16）
    #[serde(default = "default_hires_measures_per_group")]
    pub hires_measures_per_group: u32,
    /// 高精度洋葱皮贴图：贴图宽度像素（480-7680）
    #[serde(default = "default_hires_tile_width")]
    pub hires_tile_width_px: u32,
    /// 高精度洋葱皮贴图：编辑后重生成冷静期秒数（3-60）
    #[serde(default = "default_hires_cooldown")]
    pub hires_cooldown_secs: u64,
    /// 高精度洋葱皮贴图：GPU 显存上限 MB（128-4096）
    #[serde(default = "default_hires_gpu_mem_limit")]
    pub hires_gpu_mem_limit_mb: u32,
    /// 播放时键盘颜色指示（默认关闭以节省内存和性能）
    #[serde(default)]
    pub playback_key_colors_enabled: bool,
    /// 添加音轨时的行为（自动跳转到新音轨 / 保持当前音轨）
    #[serde(default)]
    pub track_add_behavior: TrackAddBehavior,
    /// 当前选中的调色板名称（空字符串表示使用默认）
    #[serde(default)]
    pub selected_palette: String,
    /// 编辑历史：操作日志总条数上限（默认 100）
    #[serde(default = "default_history_total_limit")]
    pub history_total_limit: usize,
    /// 编辑历史：单条日志条目上限（默认 1000，超限自动分割）
    #[serde(default = "default_history_entry_limit")]
    pub history_entry_limit: usize,
    /// 编辑历史：合并窗口毫秒数（仅 Pencil 绘制，默认 300ms，0=不合并）
    #[serde(default = "default_merge_window_ms")]
    pub merge_window_ms: u64,
    /// 编辑拦截：是否显示 Toast 提示（默认 true）
    #[serde(default = "default_true")]
    pub intercept_notification_enabled: bool,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）
    #[serde(default = "default_automation_line_thickness")]
    pub automation_line_thickness: f32,
    /// Tempo 面板 BPM 绘制上限（默认 512，可配置 256~65536 或自定义）
    #[serde(default = "default_tempo_max_bpm")]
    pub tempo_max_bpm: f64,
    /// 日志文件保留份数（默认 10，0 = 不限制）
    #[serde(default = "default_log_retention_count")]
    pub log_retention_count: usize,
    /// 底边栏监控数据刷新间隔（毫秒，50-2000，默认 100）
    #[serde(default = "default_monitor_refresh_interval_ms")]
    pub monitor_refresh_interval_ms: f32,
    /// 音频引擎后端（Realtime vs Core）
    #[serde(default)]
    pub audio_engine: AudioEngineKind,
    /// Core 引擎环形缓冲目标帧数（512..16384，默认 4096≈85ms@48k）
    #[serde(default = "default_core_buffer_frames")]
    pub core_buffer_frames: u32,
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
fn default_automation_line_thickness() -> f32 {
    2.0
}

/// Tempo 面板 BPM 绘制上限默认值
fn default_tempo_max_bpm() -> f64 {
    512.0
}

fn default_monitor_refresh_interval_ms() -> f32 {
    100.0
}

fn default_core_buffer_frames() -> u32 {
    4096
}

fn default_log_retention_count() -> usize {
    10
}

fn default_velocity_filter_threshold() -> u8 {
    1
}

fn default_hires_measures_per_group() -> u32 {
    4
}
fn default_hires_tile_width() -> u32 {
    1920
}
fn default_hires_cooldown() -> u64 {
    10
}
fn default_hires_gpu_mem_limit() -> u32 {
    512
}

fn default_history_total_limit() -> usize {
    100
}

fn default_history_entry_limit() -> usize {
    1000
}

fn default_merge_window_ms() -> u64 {
    300
}
/// 用户界面配置默认值
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
            language: Language::default(),
            preferred_backend: SynthBackend::XSynth,
            soundfont_path: String::new(),
            use_native_titlebar: false,
            xsynth_buffer_ms: default_synth_buffer(),
            xsynth_sample_rate: default_synth_sample_rate(),
            xsynth_threads: default_synth_threads(),
            xsynth_fade_out_killing: default_synth_fade_out(),
            xsynth_max_voices_per_key: default_max_voices_per_key(),
            selection_box_mode: SelectionBoxMode::default(),
            eraser_behavior: EraserBehavior::default(),
            program_font_name: String::from("Microsoft YaHei"),
            program_font_path: String::new(),
            auto_scroll: AutoScrollConfig::default(),
            velocity_filter_threshold: default_velocity_filter_threshold(),
            xsynth_global_voice_limit: None,
            icon_hidpi: true,
            enable_256key: false,
            velocity_curve_style: true,
            hires_onion_enabled: true,
            hires_measures_per_group: default_hires_measures_per_group(),
            hires_tile_width_px: default_hires_tile_width(),
            hires_cooldown_secs: default_hires_cooldown(),
            hires_gpu_mem_limit_mb: default_hires_gpu_mem_limit(),
            playback_key_colors_enabled: false,
            track_add_behavior: TrackAddBehavior::default(),
            selected_palette: String::new(),
            history_total_limit: default_history_total_limit(),
            history_entry_limit: default_history_entry_limit(),
            merge_window_ms: default_merge_window_ms(),
            intercept_notification_enabled: true,
            automation_line_thickness: default_automation_line_thickness(),
            tempo_max_bpm: default_tempo_max_bpm(),
            log_retention_count: default_log_retention_count(),
            monitor_refresh_interval_ms: default_monitor_refresh_interval_ms(),
            audio_engine: AudioEngineKind::default(),
            core_buffer_frames: default_core_buffer_frames(),
        }
    }
}
