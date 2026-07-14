//! 视频导出配置模型
//!
//! 移植自 nezha-encoder，剔除音频相关字段。
//! 定义容器格式、视频编码器、硬件加速后端、质量预设等枚举与配置结构体。

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// 容器格式
// ---------------------------------------------------------------------------

/// 输出容器格式
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    Avi,
}

impl Container {
    /// 文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "mkv",
            Container::Avi => "avi",
        }
    }

    /// ffmpeg 封装器名称
    pub fn ffmpeg_muxer(&self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "matroska",
            Container::Avi => "avi",
        }
    }
}

impl std::str::FromStr for Container {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "MP4" => Ok(Container::Mp4),
            "MOV" => Ok(Container::Mov),
            "MKV" => Ok(Container::Mkv),
            "AVI" => Ok(Container::Avi),
            _ => Err(format!("未知容器格式: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// 视频编码器
// ---------------------------------------------------------------------------

/// 视频编码器
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    ProRes,
    Vp9,
    Av1,
}

impl VideoCodec {
    /// ffmpeg 编码器名短标识（如 "h264"、"hevc"）
    pub fn ffmpeg_codec_name(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "hevc",
            VideoCodec::ProRes => "prores",
            VideoCodec::Vp9 => "vp9",
            VideoCodec::Av1 => "av1",
        }
    }

    /// 软件编码器名（如 "libx264"、"libx265"）
    pub fn ffmpeg_software_encoder(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::ProRes => "prores_ks",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Av1 => "libsvtav1",
        }
    }

    /// 输出像素格式
    pub fn ffmpeg_pix_fmt(&self) -> &'static str {
        match self {
            VideoCodec::ProRes => "yuv422p",
            _ => "yuv420p",
        }
    }

    /// UI 下拉框显示名
    pub fn display_name(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::H265 => "H.265 / HEVC",
            VideoCodec::ProRes => "ProRes",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1",
        }
    }
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
            _ => Err(format!("未知视频编码器: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// 硬件加速后端
// ---------------------------------------------------------------------------

/// 硬件加速后端（按平台过滤可用项）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncoderBackend {
    /// 纯软件编码
    Software,
    /// macOS VideoToolbox
    VideoToolbox,
    /// NVIDIA NVENC（Windows/Linux）
    Nvenc,
    /// AMD AMF（Windows）
    Amf,
    /// Intel QuickSync（Windows/Linux）
    Qsv,
    /// VAAPI（Linux）
    Vaapi,
}

impl EncoderBackend {
    /// ffmpeg 编码器名后缀（如 "videotoolbox"、"nvenc"）
    pub fn ffmpeg_suffix(&self) -> Option<&'static str> {
        match self {
            EncoderBackend::Software => None,
            EncoderBackend::VideoToolbox => Some("videotoolbox"),
            EncoderBackend::Nvenc => Some("nvenc"),
            EncoderBackend::Amf => Some("amf"),
            EncoderBackend::Qsv => Some("qsv"),
            EncoderBackend::Vaapi => Some("vaapi"),
        }
    }

    /// 是否为硬件加速后端
    pub fn is_hardware(&self) -> bool {
        !matches!(self, EncoderBackend::Software)
    }

    /// UI 显示名
    pub fn display_name(&self) -> &'static str {
        match self {
            EncoderBackend::Software => "Software (CPU)",
            EncoderBackend::VideoToolbox => "VideoToolbox (macOS)",
            EncoderBackend::Nvenc => "NVENC (NVIDIA)",
            EncoderBackend::Amf => "AMF (AMD)",
            EncoderBackend::Qsv => "QSV (Intel)",
            EncoderBackend::Vaapi => "VAAPI (Linux)",
        }
    }

    /// 返回当前操作系统可用的后端列表
    pub fn available_on_current_platform() -> Vec<EncoderBackend> {
        use EncoderBackend::*;
        let mut list = vec![Software];
        // macOS
        #[cfg(target_os = "macos")]
        list.push(VideoToolbox);
        // Windows
        #[cfg(target_os = "windows")]
        {
            list.push(Nvenc);
            list.push(Amf);
            list.push(Qsv);
        }
        // Linux
        #[cfg(target_os = "linux")]
        {
            list.push(Nvenc);
            list.push(Qsv);
            list.push(Vaapi);
        }
        list
    }
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
            _ => Err(format!("未知编码后端: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// 质量预设
// ---------------------------------------------------------------------------

/// 质量预设（影响 CRF / 码率 / preset）
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum QualityPreset {
    High,
    #[default]
    Medium,
    Low,
}

impl QualityPreset {
    /// CRF 值（软件编码器用）
    pub fn crf(&self) -> &'static str {
        match self {
            QualityPreset::High => "18",
            QualityPreset::Medium => "23",
            QualityPreset::Low => "28",
        }
    }

    /// preset 值（编码速度/质量权衡）
    pub fn preset(&self) -> &'static str {
        match self {
            QualityPreset::High => "slow",
            QualityPreset::Medium => "medium",
            QualityPreset::Low => "veryfast",
        }
    }

    /// UI 显示名
    pub fn display_name(&self) -> &'static str {
        match self {
            QualityPreset::High => "高",
            QualityPreset::Medium => "中",
            QualityPreset::Low => "低",
        }
    }
}

impl std::str::FromStr for QualityPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "高" => Ok(QualityPreset::High),
            "中" => Ok(QualityPreset::Medium),
            "低" => Ok(QualityPreset::Low),
            _ => Err(format!("未知质量预设: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// 视频导出配置
// ---------------------------------------------------------------------------

/// 视频导出配置（纯视频，不含音频）
pub struct VideoExportConfig {
    /// 视频宽度（像素）
    pub width: u32,
    /// 视频高度（像素）
    pub height: u32,
    /// 帧率
    pub fps: f64,
    /// 容器格式
    pub container: Container,
    /// 视频编码器
    pub codec: VideoCodec,
    /// 硬件加速后端
    pub backend: EncoderBackend,
    /// 输出文件路径
    pub output_path: PathBuf,
    /// 质量预设
    pub quality: QualityPreset,
}

impl VideoExportConfig {
    /// 根据时长（秒）计算总帧数
    pub fn total_frames(&self, duration_secs: f64) -> u64 {
        (duration_secs * self.fps).ceil() as u64
    }

    /// 根据 编码器 + 后端 组装 ffmpeg 编码器名
    ///
    /// 示例: "libx264"、"h264_videotoolbox"、"hevc_nvenc"
    pub fn ffmpeg_encoder_name(&self) -> String {
        match &self.backend {
            EncoderBackend::Software => self.codec.ffmpeg_software_encoder().to_string(),
            _ => {
                let codec = self.codec.ffmpeg_codec_name();
                let suffix = self.backend.ffmpeg_suffix().expect("硬件后端应有后缀");
                format!("{}_{}", codec, suffix)
            }
        }
    }
}
