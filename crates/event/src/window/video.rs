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
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "MP4" => Ok(Container::Mp4),
            "MOV" => Ok(Container::Mov),
            "MKV" => Ok(Container::Mkv),
            "AVI" => Ok(Container::Avi),
            _ => Err(format!("未知容器格式: {input}")),
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
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "H.264" => Ok(VideoCodec::H264),
            "H.265 / HEVC" => Ok(VideoCodec::H265),
            "ProRes" => Ok(VideoCodec::ProRes),
            "VP9" => Ok(VideoCodec::Vp9),
            "AV1" => Ok(VideoCodec::Av1),
            _ => Err(format!("未知视频编码器: {input}")),
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
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "Software (CPU)" => Ok(EncoderBackend::Software),
            "VideoToolbox (macOS)" => Ok(EncoderBackend::VideoToolbox),
            "NVENC (NVIDIA)" => Ok(EncoderBackend::Nvenc),
            "AMF (AMD)" => Ok(EncoderBackend::Amf),
            "QSV (Intel)" => Ok(EncoderBackend::Qsv),
            "VAAPI (Linux)" => Ok(EncoderBackend::Vaapi),
            _ => Err(format!("未知编码后端: {input}")),
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
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "高" => Ok(QualityPreset::High),
            "中" => Ok(QualityPreset::Medium),
            "低" => Ok(QualityPreset::Low),
            _ => Err(format!("未知质量预设: {input}")),
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
    /// MIDITrail 风格（3D MIDI 轨迹可视化）
    MIDITrail,
    /// 计数器渲染（不绘制卷帘，仅在画面上显示变化的统计数据文本）
    NoteCounter,
}

impl RenderMode {
    /// 导出到渲染线程用的规范字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Waterfall => "waterfall",
            RenderMode::NoteRectangle => "note_rectangle",
            RenderMode::MIDITrail => "miditrail",
            RenderMode::NoteCounter => "note_counter",
        }
    }
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderMode::Waterfall => f.write_str("Lumino瀑布流"),
            RenderMode::NoteRectangle => f.write_str("音符矩形"),
            RenderMode::MIDITrail => f.write_str("MIDITrail"),
            RenderMode::NoteCounter => f.write_str("计数器"),
        }
    }
}

impl std::str::FromStr for RenderMode {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "waterfall" | "瀑布流" | "Lumino瀑布流" => Ok(RenderMode::Waterfall),
            "note_rectangle" | "音符矩形" => Ok(RenderMode::NoteRectangle),
            "miditrail" | "MIDITrail" => Ok(RenderMode::MIDITrail),
            "note_counter" | "计数器" | "NoteCounter" => Ok(RenderMode::NoteCounter),
            _ => Err(format!("未知渲染模式: {input}")),
        }
    }
}

/// 计数器文本对齐方式（参考 Zenith-MIDI NoteCountRender 的六种对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CounterAlignment {
    /// 左上角
    #[default]
    TopLeft,
    /// 右上角
    TopRight,
    /// 左下角
    BottomLeft,
    /// 右下角
    BottomRight,
    /// 顶部垂直均匀分布
    TopSpread,
    /// 底部垂直均匀分布
    BottomSpread,
}

impl CounterAlignment {
    /// 设置面板用的规范字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            CounterAlignment::TopLeft => "top_left",
            CounterAlignment::TopRight => "top_right",
            CounterAlignment::BottomLeft => "bottom_left",
            CounterAlignment::BottomRight => "bottom_right",
            CounterAlignment::TopSpread => "top_spread",
            CounterAlignment::BottomSpread => "bottom_spread",
        }
    }
}

impl std::fmt::Display for CounterAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterAlignment::TopLeft => f.write_str("左上"),
            CounterAlignment::TopRight => f.write_str("右上"),
            CounterAlignment::BottomLeft => f.write_str("左下"),
            CounterAlignment::BottomRight => f.write_str("右下"),
            CounterAlignment::TopSpread => f.write_str("顶部分散"),
            CounterAlignment::BottomSpread => f.write_str("底部分散"),
        }
    }
}

impl std::str::FromStr for CounterAlignment {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "top_left" | "左上" => Ok(CounterAlignment::TopLeft),
            "top_right" | "右上" => Ok(CounterAlignment::TopRight),
            "bottom_left" | "左下" => Ok(CounterAlignment::BottomLeft),
            "bottom_right" | "右下" => Ok(CounterAlignment::BottomRight),
            "top_spread" | "顶部分散" => Ok(CounterAlignment::TopSpread),
            "bottom_spread" | "底部分散" => Ok(CounterAlignment::BottomSpread),
            _ => Err(format!("未知对齐方式: {input}")),
        }
    }
}

/// 千分位分隔符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CounterSeparator {
    /// 逗号分隔（1,234,567）
    #[default]
    Comma,
    /// 无分隔符
    Nothing,
}

impl CounterSeparator {
    pub fn as_str(&self) -> &'static str {
        match self {
            CounterSeparator::Comma => "comma",
            CounterSeparator::Nothing => "nothing",
        }
    }
}

impl std::fmt::Display for CounterSeparator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterSeparator::Comma => f.write_str("逗号"),
            CounterSeparator::Nothing => f.write_str("无"),
        }
    }
}

impl std::str::FromStr for CounterSeparator {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "comma" | "逗号" => Ok(CounterSeparator::Comma),
            "nothing" | "无" => Ok(CounterSeparator::Nothing),
            _ => Err(format!("未知千分位分隔符: {input}")),
        }
    }
}

/// 计数器文本字体来源。
///
/// - [`CounterFont::Bitmap`]：内置 5x7 点阵字体（无外部依赖，仅支持 ASCII）
/// - [`CounterFont::System`]：操作系统自带字体（如微软雅黑，支持中文等 Unicode）
/// - [`CounterFont::File`]：用户指定的 TTF/OTF/TTC 字体文件
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CounterFont {
    /// 内置 5x7 点阵字体（仅 ASCII，默认）
    #[default]
    Bitmap,
    /// 系统字体（按名称查系统字体路径表）
    System {
        /// 字体名称（如 "微软雅黑"）
        family: String,
    },
    /// 自定义字体文件（TTF/OTF/TTC）
    File {
        /// 字体文件路径
        path: String,
    },
}

impl CounterFont {
    /// 设置面板用的规范字符串（与 `FromStr` 对应）
    pub fn as_str(&self) -> &'static str {
        match self {
            CounterFont::Bitmap => "bitmap",
            CounterFont::System { .. } => "system",
            CounterFont::File { .. } => "file",
        }
    }
}

impl std::fmt::Display for CounterFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterFont::Bitmap => f.write_str("内置点阵（5x7）"),
            CounterFont::System { family } => write!(f, "系统字体：{family}"),
            CounterFont::File { path } => write!(f, "自定义字体：{path}"),
        }
    }
}

impl std::str::FromStr for CounterFont {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "bitmap" | "内置点阵" => Ok(CounterFont::Bitmap),
            "system" | "系统字体" => Ok(CounterFont::System {
                family: "微软雅黑".to_string(),
            }),
            "file" | "自定义字体" => Ok(CounterFont::File {
                path: String::new(),
            }),
            _ => Err(format!("未知字体来源: {input}")),
        }
    }
}

/// 计数器渲染配置（事件层传输结构）。
///
/// 参考 Zenith-MIDI NoteCountRender / fmr NoteCounter 的设置模型：
/// 文本模板 + 对齐 + 字号 + 千分位 + 补零 + CSV 导出 + 数字补零宽度。
#[derive(Debug, Clone)]
pub struct NoteCounterConfig {
    /// 文本模板（支持 `{nc}` `{nps}` `{bpm}` 等占位符，`\n` 换行）
    pub text: String,
    /// 文本对齐方式
    pub alignment: CounterAlignment,
    /// 字体大小（像素）
    pub font_size: u32,
    /// 字体来源（内置点阵 / 系统字体 / 自定义字体文件）
    pub font: CounterFont,
    /// 千分位分隔符
    pub separator: CounterSeparator,
    /// 数字补零（启用后按各 pad 宽度左补零）
    pub padding_zeroes: bool,
    /// BPM 整数部分补零宽度
    pub bpm_int_pad: u32,
    /// BPM 小数部分位数
    pub bpm_dec_pad: u32,
    /// 音符数补零宽度
    pub note_count_pad: u32,
    /// 复音数补零宽度
    pub polyphony_pad: u32,
    /// NPS 补零宽度
    pub nps_pad: u32,
    /// 时钟 tick 补零宽度
    pub ticks_pad: u32,
    /// 小节数补零宽度
    pub bars_pad: u32,
    /// 帧数补零宽度
    pub frames_pad: u32,
    /// 是否将每帧统计数据写入 CSV 文件
    pub save_csv: bool,
    /// CSV 输出路径
    pub csv_output: String,
    /// CSV 每行格式（支持与文本模板相同的占位符）
    pub csv_format: String,
}

impl Default for NoteCounterConfig {
    fn default() -> Self {
        Self {
            text: "Notes: {nc} / {tn}\nBPM: {bpm}\nNPS: {nps}\nPPQ: {ppq}\nPolyphony: {plph}\nTime: {currtime}".to_string(),
            alignment: CounterAlignment::TopLeft,
            font_size: 40,
            font: CounterFont::Bitmap,
            separator: CounterSeparator::Comma,
            padding_zeroes: false,
            bpm_int_pad: 3,
            bpm_dec_pad: 2,
            note_count_pad: 5,
            polyphony_pad: 3,
            nps_pad: 3,
            ticks_pad: 5,
            bars_pad: 3,
            frames_pad: 5,
            save_csv: false,
            csv_output: String::new(),
            csv_format: "{nps},{plph},{bpm},{nc}".to_string(),
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
    /// 渲染模式（瀑布流/音符矩形/MIDITrail/计数器）
    pub render_mode: RenderMode,
    /// 瀑布流滚动速度（0.1~10.0，默认 1.0）
    pub waterfall_scroll_speed: f32,
    /// MIDITrail Z 方向显示距离（0.1~15.0，默认 7.5，精度 0.1）
    pub miditrail_z_far: f32,
    /// 计数器渲染配置（仅 `render_mode == NoteCounter` 时生效）
    pub note_counter: NoteCounterConfig,
}
