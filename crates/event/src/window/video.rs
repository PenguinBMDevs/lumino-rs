//! 视频导出事件共享类型。
//!
//! 事件层传输结构：与 `lumino-export::video::config` 的强类型枚举同构，
//! 但事件层不依赖导出实现层（依赖方向：export < event），故在此独立定义，
//! 由 runner 做 1:1 映射到导出层枚举。

/// 输出容器格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Container {
    #[default]
    Mp4,
    Mov,
    Mkv,
    Avi,
}

impl std::str::FromStr for Container {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "MP4" => Ok(Container::Mp4),
            "MOV" => Ok(Container::Mov),
            "MKV" => Ok(Container::Mkv),
            "AVI" => Ok(Container::Avi),
            _ => Err(format!("未知容器格式: {s}")),
        }
    }
}

/// 视频编码器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoCodec {
    #[default]
    H264,
    H265,
    ProRes,
    Vp9,
    Av1,
}

impl std::str::FromStr for VideoCodec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "H.264" => Ok(VideoCodec::H264),
            "H.265 / HEVC" => Ok(VideoCodec::H265),
            "ProRes" => Ok(VideoCodec::ProRes),
            "VP9" => Ok(VideoCodec::Vp9),
            "AV1" => Ok(VideoCodec::Av1),
            _ => Err(format!("未知视频编码器: {s}")),
        }
    }
}

/// 硬件加速后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncoderBackend {
    #[default]
    Software,
    VideoToolbox,
    Nvenc,
    Amf,
    Qsv,
    Vaapi,
}

impl std::str::FromStr for EncoderBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Software (CPU)" => Ok(EncoderBackend::Software),
            "VideoToolbox (macOS)" => Ok(EncoderBackend::VideoToolbox),
            "NVENC (NVIDIA)" => Ok(EncoderBackend::Nvenc),
            "AMF (AMD)" => Ok(EncoderBackend::Amf),
            "QSV (Intel)" => Ok(EncoderBackend::Qsv),
            "VAAPI (Linux)" => Ok(EncoderBackend::Vaapi),
            _ => Err(format!("未知编码后端: {s}")),
        }
    }
}

/// 质量预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityPreset {
    High,
    #[default]
    Medium,
    Low,
}

impl std::str::FromStr for QualityPreset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "高" => Ok(QualityPreset::High),
            "中" => Ok(QualityPreset::Medium),
            "低" => Ok(QualityPreset::Low),
            _ => Err(format!("未知质量预设: {s}")),
        }
    }
}

/// 视频导出渲染模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Lumino瀑布流渲染（默认模式，音符随时间向下流动）
    #[default]
    Waterfall,
    /// 音符矩形渲染（传统钢琴卷帘样式）
    NoteRectangle,
    /// Comet Enhanced 风格（3D增强渲染，发光/粒子效果）
    Enhanced,
    /// Comet MIDITrail 风格（MIDI轨迹可视化）
    MIDITrail,
    /// Comet PFA 风格（Piano Forte力度渲染）
    PFA,
    /// Comet Velocities 风格（力度可视化渲染）
    Velocities,
    /// Comet Channels 风格（通道分离渲染）
    Channels,
}

impl RenderMode {
    /// 导出到渲染线程用的规范字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Waterfall => "waterfall",
            RenderMode::NoteRectangle => "note_rectangle",
            RenderMode::Enhanced => "enhanced",
            RenderMode::MIDITrail => "miditrail",
            RenderMode::PFA => "pfa",
            RenderMode::Velocities => "velocities",
            RenderMode::Channels => "channels",
        }
    }
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderMode::Waterfall => f.write_str("Lumino瀑布流"),
            RenderMode::NoteRectangle => f.write_str("音符矩形"),
            RenderMode::Enhanced => f.write_str("Enhanced"),
            RenderMode::MIDITrail => f.write_str("MIDITrail"),
            RenderMode::PFA => f.write_str("PFA"),
            RenderMode::Velocities => f.write_str("Velocities"),
            RenderMode::Channels => f.write_str("Channels"),
        }
    }
}

impl std::str::FromStr for RenderMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "waterfall" | "瀑布流" | "Lumino瀑布流" => Ok(RenderMode::Waterfall),
            "note_rectangle" | "音符矩形" => Ok(RenderMode::NoteRectangle),
            "enhanced" | "Enhanced" => Ok(RenderMode::Enhanced),
            "miditrail" | "MIDITrail" => Ok(RenderMode::MIDITrail),
            "pfa" | "PFA" => Ok(RenderMode::PFA),
            "velocities" | "Velocities" => Ok(RenderMode::Velocities),
            "channels" | "Channels" => Ok(RenderMode::Channels),
            _ => Err(format!("未知渲染模式: {s}")),
        }
    }
}

/// 视频导出配置（事件层传输结构）。
#[derive(Debug, Clone)]
pub struct VideoExportConfig {
    /// 输出文件路径
    pub output_path: String,
    /// MIDI 文件路径（流式读取模式使用）
    pub midi_path: String,
    /// 视频宽度（像素）
    pub width: u32,
    /// 视频高度（像素）
    pub height: u32,
    /// 帧率
    pub fps: u32,
    /// MIDI 分辨率（PPQ）
    pub ppq: u16,
    /// 可见键位数（128 或 256，用于 Y 向缩放）
    pub key_count: u16,
    /// 容器格式
    pub container: Container,
    /// 视频编码器
    pub codec: VideoCodec,
    /// 硬件加速后端
    pub backend: EncoderBackend,
    /// 质量预设
    pub quality: QualityPreset,
    /// 渲染模式（瀑布流/音符矩形）
    pub render_mode: RenderMode,
    /// 瀑布流滚动速度（0.1~10.0，默认 1.0）
    pub waterfall_scroll_speed: f32,
}
