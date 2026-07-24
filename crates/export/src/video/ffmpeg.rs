//! FFmpeg 编码器封装
//!
//! 移植自 nezha-encoder/ffmpeg.rs，剔除音频 WAV 逻辑，仅保留纯视频流编码。
//!
//! 核心流程：以子进程方式启动 ffmpeg，通过 stdin pipe 喂入 BGRA rawvideo 帧，
//! ffmpeg 内部完成 BGRA→YUV420p 转换与封装。使用有界 channel 做背压控制，
//! stderr 独立线程捕获用于错误诊断，Drop 时自动 kill 进程。

use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::video::config::{EncoderBackend, QualityPreset, VideoCodec, VideoExportConfig};
use crate::video::error::{VideoExportError, VideoExportResult};

/// FFmpeg 编码器
///
/// 持有 ffmpeg 子进程与 `stdin` 写入缓冲。当前为**直连写入模式**：
/// 调用方线程直接调用 `write_frame` 将帧数据写入 ffmpeg stdin，
/// 消除跨线程 channel 跳转与额外上下文切换。Drop 时自动 kill 进程（用户取消场景）。
#[derive(Debug)]
pub struct FfmpegEncoder {
    /// 帧数据写入缓冲（1 MB），持有 ffmpeg stdin pipe
    stdin_writer: Option<BufWriter<ChildStdin>>,
    /// ffmpeg 子进程
    process: std::process::Child,
    /// 帧尺寸（BGRA/RGBA，width*height*4 字节）
    width: u32,
    height: u32,
    /// ffmpeg stderr 捕获缓冲（用于错误诊断）
    stderr_buf: Arc<Mutex<Vec<String>>>,
    /// 写入时的 IO 错误（若有）
    writer_error: Arc<Mutex<Option<String>>>,
}

impl FfmpegEncoder {
    /// 创建并启动 ffmpeg 编码器（直连写入模式）
    ///
    /// 内部完成：ffmpeg 路径解析、参数组装、子进程启动、stderr 捕获线程、
    /// `stdin` 写入缓冲初始化。当前版本不在编码器内部启动额外线程，
    /// 调用方直接调用 `write_frame` 写入 pipe，消除 crossbeam channel 跳转。
    ///
    /// `input_pix_fmt` 为原始帧数据的像素格式（如 `"bgra"` 或 `"rgba"`），
    /// 需与 GPU 离屏纹理的实际通道顺序一致。
    pub fn new(config: &VideoExportConfig, input_pix_fmt: &'static str) -> VideoExportResult<Self> {
        let ffmpeg = ffmpeg_path()?;

        tracing::info!(path = %ffmpeg.display(), "启动 ffmpeg 编码器（直连写入模式）");

        let args = build_ffmpeg_args(config, input_pix_fmt);
        tracing::debug!(?args, "ffmpeg 参数");

        let mut process = Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // 共享错误诊断缓冲
        let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let writer_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // stderr 捕获线程：实时读取 ffmpeg stderr
        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| VideoExportError::PipeSetupFailed("ffmpeg stderr 未 piped".into()))?;
        let stderr_buf_clone = stderr_buf.clone();
        thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim_end().to_string();
                        if !trimmed.is_empty() {
                            tracing::warn!("[ffmpeg] {}", trimmed);
                            if let Ok(mut buf) = stderr_buf_clone.lock() {
                                buf.push(trimmed);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[ffmpeg] stderr 读取错误: {}", e);
                        break;
                    }
                }
            }
        });

        let stdin_writer = Some(BufWriter::with_capacity(
            1024 * 1024, // 1MB 缓冲，批量写入
            process
                .stdin
                .take()
                .ok_or_else(|| VideoExportError::PipeSetupFailed("ffmpeg stdin 未 piped".into()))?,
        ));

        Ok(Self {
            stdin_writer,
            process,
            width: config.width,
            height: config.height,
            stderr_buf,
            writer_error,
        })
    }

    /// 写入一帧 BGRA 数据（width×height×4 字节）
    ///
    /// 原始 BGRA 数据直接写入 ffmpeg stdin pipe。成功后将 `frame_data` 原样返回，
    /// 方便调用方将其归还到对象池复用；失败时数据被丢弃。
    pub fn write_frame(&mut self, frame_data: Vec<u8>) -> VideoExportResult<Vec<u8>> {
        let expected = (self.width * self.height * 4) as usize;
        if frame_data.len() != expected {
            return Err(VideoExportError::FrameSizeMismatch {
                got: frame_data.len(),
                expected,
            });
        }

        let Some(writer) = self.stdin_writer.as_mut() else {
            return Err(self.build_write_error("stdin_writer 已关闭，无法写入帧"));
        };
        if let Err(e) = writer.write_all(&frame_data) {
            let msg = format!("write_all 失败: {e}");
            if let Ok(mut err) = self.writer_error.lock() {
                *err = Some(msg);
            }
            return Err(VideoExportError::Io(e));
        }

        Ok(frame_data)
    }

    /// 完成编码：flush 并关闭 stdin → 等待 ffmpeg 进程退出
    ///
    /// 消费 self，成功返回 Ok(())，失败返回含 stderr 上下文的错误。
    pub fn finish(mut self) -> VideoExportResult<()> {
        // 显式 take 并 drop stdin_writer，触发 flush 并关闭 pipe，让 ffmpeg 收到 EOF
        if let Some(writer) = self.stdin_writer.take() {
            std::mem::drop(writer);
        }

        // 等待 ffmpeg 进程
        let status = self.process.wait()?;
        if !status.success() {
            let stderr_lines = self.stderr_content();
            tracing::error!(
                code = status.code(),
                stderr = %stderr_lines,
                "ffmpeg 非零退出"
            );
            let msg = match status.code() {
                Some(code) => format!("ffmpeg 退出码 {code}"),
                None => "ffmpeg 未知错误退出".to_string(),
            };
            return Err(VideoExportError::FfmpegWriteFailed(format!(
                "{msg}\nffmpeg stderr:\n{stderr_lines}"
            )));
        }

        tracing::info!("ffmpeg 编码完成");
        Ok(())
    }

    /// 拼接捕获的 stderr 行
    fn stderr_content(&self) -> String {
        self.stderr_buf
            .lock()
            .map(|buf| buf.join("\n"))
            .unwrap_or_default()
    }

    /// 构建带 writer 错误与 stderr 上下文的 FfmpegWriteFailed
    fn build_write_error(&self, context: &str) -> VideoExportError {
        let mut parts = vec![context.to_string()];
        if let Ok(writer_err) = self.writer_error.lock()
            && let Some(ref e) = *writer_err
        {
            parts.push(format!("写入错误: {e}"));
        }
        let stderr = self.stderr_content();
        if !stderr.is_empty() {
            parts.push(format!("ffmpeg stderr:\n{stderr}"));
        }
        VideoExportError::FfmpegWriteFailed(parts.join("\n"))
    }
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        // 先 take/drop stdin_writer，确保未写入数据 flush 并关闭 pipe 后再 kill
        if let Some(writer) = self.stdin_writer.take() {
            std::mem::drop(writer);
        }
        // 用户取消时 kill ffmpeg 进程
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

// ---------------------------------------------------------------------------
// ffmpeg 可执行文件发现
// ---------------------------------------------------------------------------

/// 返回 ffmpeg 可执行文件路径
///
/// 检查顺序：
///   1. 可执行文件同目录下的 ffmpeg（随程序分发的版本）
///   2. PATH 中的 ffmpeg
///
/// 只要两者之一可用即可导出。
pub fn ffmpeg_path() -> VideoExportResult<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    // 1. 程序目录下的 ffmpeg（bundled）
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join(exe_name);
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    // 2. PATH 中的 ffmpeg
    if Command::new(exe_name)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return Ok(PathBuf::from(exe_name));
    }

    Err(VideoExportError::FfmpegNotFound)
}

/// 检查 ffmpeg 是否可用（用于 UI 提前检测）
pub fn is_ffmpeg_available() -> bool {
    ffmpeg_path().is_ok()
}

// ---------------------------------------------------------------------------
// ffmpeg 参数构建
// ---------------------------------------------------------------------------

/// 组装 ffmpeg 命令行参数（纯视频流，无音频）
fn build_ffmpeg_args(config: &VideoExportConfig, input_pix_fmt: &str) -> Vec<String> {
    let mut args = Vec::new();

    // ── 视频输入：stdin raw BGRA/RGBA ──
    // ffmpeg 内部完成 BGRA/RGBA→YUV 转换
    args.push("-f".to_string());
    args.push("rawvideo".to_string());
    args.push("-pix_fmt".to_string());
    args.push(input_pix_fmt.to_string());
    args.push("-s".to_string());
    args.push(format!("{}x{}", config.width, config.height));
    args.push("-r".to_string());
    args.push(format!("{:.3}", config.fps));
    // 限制 ffmpeg 内部队列，防止编码速度跟不上时堆积数 GB 内存
    args.push("-thread_queue_size".to_string());
    args.push("8".to_string());
    args.push("-i".to_string());
    args.push("-".to_string());

    // ── 多线程 ──
    args.push("-threads".to_string());
    args.push("0".to_string());

    // ── 视频编码器 ──
    args.push("-c:v".to_string());
    args.push(config.ffmpeg_encoder_name());

    // ── 色彩范围：强制全范围 PC，防止暗化 ──
    // ffmpeg 默认 YUV 输出为 limited range (16-235)，会压缩亮度导致偏色
    args.push("-color_range".to_string());
    args.push("pc".to_string());

    // ── 后端专属质量参数 ──
    match &config.backend {
        EncoderBackend::Software => build_software_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::VideoToolbox => {
            build_videotoolbox_args(&mut args, &config.codec, &config.quality)
        }
        EncoderBackend::Nvenc => build_nvenc_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::Amf => build_amf_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::Qsv => build_qsv_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::MediaFoundation => build_mf_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::Vaapi => build_vaapi_args(&mut args, &config.codec, &config.quality),
    }

    // ── 封装格式 ──
    args.push("-f".to_string());
    args.push(config.container.ffmpeg_muxer().to_string());

    // muxing 队列：防止编码落后于封装时 OOM
    args.push("-max_muxing_queue_size".to_string());
    args.push("64".to_string());

    // 覆盖输出
    args.push("-y".to_string());

    // 输出路径
    args.push(config.output_path.to_string_lossy().to_string());

    args
}

// ---------------------------------------------------------------------------
// 后端专属质量参数
// ---------------------------------------------------------------------------

/// 软件编码器：libx264 / libx265 / prores_ks / libvpx-vp9 / libsvtav1
fn build_software_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    match codec {
        VideoCodec::H264 | VideoCodec::H265 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            args.push("-preset".to_string());
            args.push(quality.preset().to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
        }
        VideoCodec::Vp9 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
            // VP9 多线程
            args.push("-row-mt".to_string());
            args.push("1".to_string());
            args.push("-tile-columns".to_string());
            args.push("2".to_string());
        }
        VideoCodec::Av1 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
            // SVT-AV1 多线程
            args.push("-svtav1-params".to_string());
            args.push(format!("lp={}", num_cpus()));
        }
        VideoCodec::ProRes => {
            args.push("-profile:v".to_string());
            args.push("3".to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv422p".to_string());
            args.push("-qscale:v".to_string());
            args.push("9".to_string());
        }
    }
}

/// macOS VideoToolbox：h264_videotoolbox / hevc_videotoolbox / prores_videotoolbox
///
/// 使用目标码率与质量等级（1=最佳, 4=最快），而非 CRF。
fn build_videotoolbox_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    match codec {
        VideoCodec::H264 | VideoCodec::H265 => {
            let (bitrate, vt_q) = match quality {
                QualityPreset::High => ("50M", "1"),
                QualityPreset::Medium => ("20M", "2"),
                QualityPreset::Low => ("10M", "4"),
            };
            args.push("-b:v".to_string());
            args.push(bitrate.to_string());
            args.push("-quality".to_string());
            args.push(vt_q.to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv420p".to_string());
        }
        VideoCodec::ProRes => {
            let bitrate = match quality {
                QualityPreset::High => "100M",
                QualityPreset::Medium => "50M",
                QualityPreset::Low => "20M",
            };
            args.push("-b:v".to_string());
            args.push(bitrate.to_string());
            args.push("-pix_fmt".to_string());
            args.push("yuv422p".to_string());
        }
        _ => {
            // 不支持的 编码器/后端 组合 —— ffmpeg 会报错，但仍尝试
            args.push("-b:v".to_string());
            args.push("20M".to_string());
        }
    }
}

/// NVIDIA NVENC：h264_nvenc / hevc_nvenc / av1_nvenc（Windows & Linux）
///
/// 使用 -cq 恒定质量 + VBR 码率控制 + preset (p1-p7)。
fn build_nvenc_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    let cq = match quality {
        QualityPreset::High => "18",
        QualityPreset::Medium => "23",
        QualityPreset::Low => "28",
    };
    let preset = match quality {
        // p1=最快, p7=最慢
        QualityPreset::High => "p5",
        QualityPreset::Medium => "p4",
        QualityPreset::Low => "p2",
    };

    args.push("-cq".to_string());
    args.push(cq.to_string());
    args.push("-rc".to_string());
    args.push("vbr".to_string());
    args.push("-preset".to_string());
    args.push(preset.to_string());
    args.push("-pix_fmt".to_string());
    args.push(codec.ffmpeg_pix_fmt().to_string());
}

/// AMD AMF：h264_amf / hevc_amf / av1_amf（Windows）
fn build_amf_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    let (bitrate, amf_q) = match quality {
        QualityPreset::High => ("15M", "quality"),
        QualityPreset::Medium => ("8M", "balanced"),
        QualityPreset::Low => ("4M", "speed"),
    };
    args.push("-b:v".to_string());
    args.push(bitrate.to_string());
    args.push("-quality".to_string());
    args.push(amf_q.to_string());
    args.push("-pix_fmt".to_string());
    args.push(codec.ffmpeg_pix_fmt().to_string());
}

/// Intel QuickSync：h264_qsv / hevc_qsv / av1_qsv / vp9_qsv（Windows & Linux）
fn build_qsv_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    let global_q = match quality {
        QualityPreset::High => "18",
        QualityPreset::Medium => "23",
        QualityPreset::Low => "28",
    };
    let preset = match quality {
        QualityPreset::High => "medium",
        QualityPreset::Medium => "fast",
        QualityPreset::Low => "veryfast",
    };

    args.push("-global_quality".to_string());
    args.push(global_q.to_string());
    args.push("-preset".to_string());
    args.push(preset.to_string());
    args.push("-pix_fmt".to_string());
    args.push(codec.ffmpeg_pix_fmt().to_string());
}

/// Windows MediaFoundation：h264_mf / hevc_mf（Windows DXVA/D3D11）
fn build_mf_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    let bitrate = match quality {
        QualityPreset::High => "30M",
        QualityPreset::Medium => "15M",
        QualityPreset::Low => "5M",
    };
    args.push("-b:v".to_string());
    args.push(bitrate.to_string());
    args.push("-pix_fmt".to_string());
    args.push(codec.ffmpeg_pix_fmt().to_string());
}

/// VAAPI：h264_vaapi / hevc_vaapi / av1_vaapi / vp9_vaapi（Linux）
fn build_vaapi_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    let qp = match quality {
        QualityPreset::High => "18",
        QualityPreset::Medium => "23",
        QualityPreset::Low => "28",
    };
    args.push("-qp".to_string());
    args.push(qp.to_string());
    args.push("-pix_fmt".to_string());
    args.push(codec.ffmpeg_pix_fmt().to_string());
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 返回可用 CPU 核心数
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
