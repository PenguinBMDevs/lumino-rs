//! FFmpeg 编码器封装
//!
//! 移植自 nezha-encoder/ffmpeg.rs，剔除音频 WAV 逻辑，仅保留纯视频流编码。
//!
//! 核心流程：以子进程方式启动 ffmpeg，通过 stdin pipe 喂入 BGRA rawvideo 帧，
//! ffmpeg 内部完成 BGRA→YUV420p 转换与封装。使用有界 channel 做背压控制，
//! stderr 独立线程捕获用于错误诊断，Drop 时自动 kill 进程。
//!
//! 参数构建见 `args` 子模块。

mod args;

use args::*;

use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::video::config::VideoExportConfig;
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
        let stderr_buf_clone = Arc::clone(&stderr_buf);
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
