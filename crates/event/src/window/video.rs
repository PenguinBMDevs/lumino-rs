//! 视频导出事件共享类型。
//!
//! 事件层传输结构：与 `lumino-export::video::config` 的强类型枚举同构，
//! 但事件层不依赖导出实现层（依赖方向：export < event），故在此独立定义，
//! 由 runner 做 1:1 映射到导出层枚举。

/// 生成 unit-variant 枚举的 `FromStr` 实现（收敛手写样板，别名表与错误消息保留）。
///
/// `$err_fmt` 为错误消息格式串，支持 `{input}` 占位（与原手写实现一致）。
macro_rules! impl_unit_enum_from_str {
    ($ty:ident, $err_fmt:literal, { $($variant:ident => [$($alias:literal),+ $(,)?]),+ $(,)? }) => {
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(input: &str) -> Result<Self, Self::Err> {
                match input {
                    $($($alias => Ok($ty::$variant),)+)*
                    _ => Err(format!($err_fmt, input = input)),
                }
            }
        }
    };
}

/// 输出容器格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Container {
    #[default]
    Mp4,
    Mov,
    Mkv,
    Avi,
}

impl_unit_enum_from_str!(Container, "未知容器格式: {input}", {
    Mp4 => ["MP4"],
    Mov => ["MOV"],
    Mkv => ["MKV"],
    Avi => ["AVI"],
});

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

impl_unit_enum_from_str!(VideoCodec, "未知视频编码器: {input}", {
    H264 => ["H.264"],
    H265 => ["H.265 / HEVC"],
    ProRes => ["ProRes"],
    Vp9 => ["VP9"],
    Av1 => ["AV1"],
});

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

impl_unit_enum_from_str!(EncoderBackend, "未知编码后端: {input}", {
    Software => ["Software (CPU)"],
    VideoToolbox => ["VideoToolbox (macOS)"],
    Nvenc => ["NVENC (NVIDIA)"],
    Amf => ["AMF (AMD)"],
    Qsv => ["QSV (Intel)"],
    Vaapi => ["VAAPI (Linux)"],
});

/// 质量预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityPreset {
    High,
    #[default]
    Medium,
    Low,
}

impl_unit_enum_from_str!(QualityPreset, "未知质量预设: {input}", {
    High => ["高"],
    Medium => ["中"],
    Low => ["低"],
});

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
    /// 数据曲线渲染（绘制统计数据随时间的折线图，参考 MIDIGraphRenderer 移植）
    DataCurve,
}

impl RenderMode {
    /// 导出到渲染线程用的规范字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Waterfall => "waterfall",
            RenderMode::NoteRectangle => "note_rectangle",
            RenderMode::MIDITrail => "miditrail",
            RenderMode::NoteCounter => "note_counter",
            RenderMode::DataCurve => "data_curve",
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
            RenderMode::DataCurve => f.write_str("数据曲线"),
        }
    }
}

impl_unit_enum_from_str!(RenderMode, "未知渲染模式: {input}", {
    Waterfall => ["waterfall", "瀑布流", "Lumino瀑布流"],
    NoteRectangle => ["note_rectangle", "音符矩形"],
    MIDITrail => ["miditrail", "MIDITrail"],
    NoteCounter => ["note_counter", "计数器", "NoteCounter"],
    DataCurve => ["data_curve", "数据曲线", "DataCurve"],
});

/// 数据曲线模式的数据来源指标。
///
/// 原版 MIDIGraphRenderer 从 CSV 文件读入任意列数据；
/// lumino 版直接由内部统计状态按帧传入，可选四种内置指标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataCurveMetric {
    /// 每秒新开始音符数（NPS，原版默认展示数据）
    #[default]
    Nps,
    /// 当前复音数（正在发声的音符数）
    Polyphony,
    /// 累计已开始音符数
    NoteCount,
    /// 当前速度（BPM）
    Bpm,
}

impl DataCurveMetric {
    /// 设置面板用的规范字符串（与 `FromStr` 对应）
    pub fn as_str(&self) -> &'static str {
        match self {
            DataCurveMetric::Nps => "nps",
            DataCurveMetric::Polyphony => "polyphony",
            DataCurveMetric::NoteCount => "note_count",
            DataCurveMetric::Bpm => "bpm",
        }
    }
}

impl std::fmt::Display for DataCurveMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataCurveMetric::Nps => f.write_str("NPS（每秒音符数）"),
            DataCurveMetric::Polyphony => f.write_str("复音数"),
            DataCurveMetric::NoteCount => f.write_str("累计音符数"),
            DataCurveMetric::Bpm => f.write_str("BPM（速度）"),
        }
    }
}

impl_unit_enum_from_str!(DataCurveMetric, "未知数据曲线指标: {input}", {
    Nps => ["nps", "NPS", "NPS（每秒音符数）"],
    Polyphony => ["polyphony", "复音数"],
    NoteCount => ["note_count", "累计音符数"],
    Bpm => ["bpm", "BPM", "BPM（速度）"],
});

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

impl_unit_enum_from_str!(CounterAlignment, "未知对齐方式: {input}", {
    TopLeft => ["top_left", "左上"],
    TopRight => ["top_right", "右上"],
    BottomLeft => ["bottom_left", "左下"],
    BottomRight => ["bottom_right", "右下"],
    TopSpread => ["top_spread", "顶部分散"],
    BottomSpread => ["bottom_spread", "底部分散"],
});

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

impl_unit_enum_from_str!(CounterSeparator, "未知千分位分隔符: {input}", {
    Comma => ["comma", "逗号"],
    Nothing => ["nothing", "无"],
});

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

/// 数据曲线渲染配置（事件层传输结构）。
///
/// 移植自 MIDIGraphRenderer（LÖVE2D）的 graph 设置模型：
/// 自动缩放折线 + 水平刻度网格 + 里程碑文字放大 + 数字缩写。
/// 数据源由内部统计状态按帧直传（见 [`DataCurveMetric`]），不走文件 IO。
/// 颜色均为 RGBA 顺序（UI 层 hex 字符串解析后传入）。
#[derive(Debug, Clone)]
pub struct DataCurveConfig {
    /// 数据来源指标
    pub metric: DataCurveMetric,
    /// 曲线窗口时长（秒，默认 2.0，原版 `graph_duration`）
    pub graph_duration: f32,
    /// 缩放动画平滑度（EMA 分母，越大越平滑，默认 8，原版 `zoom_smoothness`）
    pub zoom_smoothness: f32,
    /// 折线前向滑动平均窗口（0=关闭，默认 0，原版 `graph_smoothness`）
    pub graph_smoothness: u32,
    /// 纵轴缩放 padding 放大系数（默认 0.1，原版 `padding_mul`）
    pub padding_mul: f32,
    /// 背景颜色（RGBA）
    pub bg_color: [u8; 4],
    /// 折线颜色（RGBA）
    pub line_color: [u8; 4],
    /// 刻度文字颜色（RGBA）
    pub text_color: [u8; 4],
    /// 水平网格线颜色（RGBA）
    pub bar_color: [u8; 4],
    /// 折线宽度（像素，默认 3）
    pub line_thickness: u32,
    /// 水平网格线宽度（像素，默认 1）
    pub bar_thickness: u32,
    /// 刻度文字字号（像素，默认 24）
    pub font_size: u32,
    /// 刻度文字字体来源（复用计数器字体渲染器）
    pub font: CounterFont,
    /// 刻度文字 X 偏移（像素，默认 2）
    pub text_x_offset: u32,
    /// 刻度文字 Y 偏移（像素，默认 2）
    pub text_y_offset: u32,
    /// 里程碑文字（1k/10k/100k…）放大倍数（默认 1.5）
    pub milestone_scale_mul: f32,
    /// 刻度数字缩写（1,000 → 1K，默认关闭）
    pub abbreviate: bool,
    /// 缩写保留小数位数（默认 3）
    pub abbreviate_digits: u32,
    /// 是否显示刻度文字（默认 true）
    pub show_text: bool,
    /// 是否显示水平网格线（默认 true）
    pub show_bars: bool,
}

impl Default for DataCurveConfig {
    fn default() -> Self {
        Self {
            metric: DataCurveMetric::Nps,
            graph_duration: 2.0,
            zoom_smoothness: 8.0,
            graph_smoothness: 0,
            padding_mul: 0.1,
            bg_color: [0, 0, 0, 255],
            line_color: [0, 255, 255, 255],
            text_color: [255, 255, 255, 127],
            bar_color: [255, 255, 255, 127],
            line_thickness: 3,
            bar_thickness: 1,
            font_size: 24,
            font: CounterFont::Bitmap,
            text_x_offset: 2,
            text_y_offset: 2,
            milestone_scale_mul: 1.5,
            abbreviate: false,
            abbreviate_digits: 3,
            show_text: true,
            show_bars: true,
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
    /// 渲染模式（瀑布流/音符矩形/MIDITrail/计数器/数据曲线）
    pub render_mode: RenderMode,
    /// 瀑布流滚动速度（0.1~10.0，默认 1.0）
    pub waterfall_scroll_speed: f32,
    /// MIDITrail Z 方向显示距离（0.1~15.0，默认 7.5，精度 0.1）
    pub miditrail_z_far: f32,
    /// 计数器渲染配置（仅 `render_mode == NoteCounter` 时生效）
    pub note_counter: NoteCounterConfig,
    /// 数据曲线渲染配置（仅 `render_mode == DataCurve` 时生效）
    pub data_curve: DataCurveConfig,
}
