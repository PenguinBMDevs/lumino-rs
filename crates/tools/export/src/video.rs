//! 视频导出模块
//!
//! 提供基于 FFmpeg 子进程的视频编码能力，接收 BGRA rawvideo 帧流。
//! 移植自 nezha-encoder，剔除音频逻辑，仅保留纯视频流编码。
//!
//! # 主要组件
//!
//! | 组件 | 说明 |
//! |------|------|
//! | [`config::VideoExportConfig`] | 导出配置（分辨率/帧率/编码器/后端/质量） |
//! | [`ffmpeg::FfmpegEncoder`] | FFmpeg 子进程封装，stdin pipe 喂帧 |
//! | [`ffmpeg::ffmpeg_path`] | ffmpeg 可执行文件发现（程序目录 + PATH） |
//! | [`error::VideoExportError`] | 错误类型 |
//!
//! # 使用流程
//!
//! 1. 构建 [`config::VideoExportConfig`]
//! 2. [`ffmpeg::FfmpegEncoder::new`] 启动编码器
//! 3. 逐帧调用 [`ffmpeg::FfmpegEncoder::write_frame`] 写入 BGRA 数据
//! 4. [`ffmpeg::FfmpegEncoder::finish`] 完成编码
//!
//! Drop 时自动 kill ffmpeg 进程（用户取消场景）。

pub mod config;
pub mod error;
pub mod ffmpeg;

// ── 配置模型 ──
pub use config::{Container, EncoderBackend, QualityPreset, VideoCodec, VideoExportConfig};

// ── 错误类型 ──
pub use error::{VideoExportError, VideoExportResult};

// ── FFmpeg 编码器 ──
pub use ffmpeg::{FfmpegEncoder, ffmpeg_path, is_ffmpeg_available};
