//! FFmpeg 编码器封装
//!
//! 移植自 nezha-encoder/ffmpeg.rs，剔除音频 WAV 逻辑，仅保留纯视频流编码。
//!
//! 核心流程：以子进程方式启动 ffmpeg，通过 stdin pipe 喂入 BGRA rawvideo 帧，
//! ffmpeg 内部完成 BGRA→YUV420p 转换与封装。stderr 由独立线程实时排空，
//! 保留有界尾部日志用于诊断，Drop 时自动 kill 进程。
//!
//! 参数构建见 `args` 子模块。

mod args;

use args::*;

use std::collections::VecDeque;
use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lumino_core::{tr, trf};

use crate::video::config::VideoExportConfig;
use crate::video::error::{VideoExportError, VideoExportResult};

const MAX_STDERR_LINES: usize = 200;
const FFMPEG_FINISH_TIMEOUT: Duration = Duration::from_secs(120);

fn push_stderr_tail(tail: &mut VecDeque<String>, line: String) {
    if tail.len() >= MAX_STDERR_LINES {
        tail.pop_front();
    }
    tail.push_back(line);
}

fn wait_for_process(process: &mut Child, timeout: Duration) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = process.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return match process.kill() {
                Ok(()) => {
                    let _ = process.wait()?;
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "ffmpeg 完成等待超时，进程已终止",
                    ))
                }
                Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => process.wait(),
                Err(e) => Err(e),
            };
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn terminate_child(process: &mut Child) {
    let _ = process.kill();
    let _ = process.wait();
}

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
    /// ffmpeg stderr 尾部缓冲（最多 200 行，用于错误诊断）
    stderr_buf: Arc<Mutex<VecDeque<String>>>,
    /// stderr 实时排空线程，进程退出后回收。
    stderr_thread: Option<thread::JoinHandle<()>>,
    /// 写入时的 IO 错误（若有）
    writer_error: Arc<Mutex<Option<String>>>,
}

impl FfmpegEncoder {
    /// 创建并启动 ffmpeg 编码器（直连写入模式）
    ///
    /// 内部完成：ffmpeg 路径解析、参数组装、子进程启动、stderr 捕获线程、
    /// `stdin` 写入缓冲初始化。帧数据由调用方线程直接写入，另设线程持续排空 stderr。
    ///
    /// `input_pix_fmt` 为原始帧数据的像素格式（如 `"bgra"` 或 `"rgba"`），
    /// 需与 GPU 离屏纹理的实际通道顺序一致。
    pub fn new(config: &VideoExportConfig, input_pix_fmt: &'static str) -> VideoExportResult<Self> {
        // 输出路径是启动外部进程的硬性前提：空串会让 ffmpeg 以
        // `Error opening output file .`（code -22）立即退出，而调用方已经
        // 白跑一轮帧生成。必须在 `ffmpeg_path()` 之前拦截，且不依赖环境。
        if config.output_path.to_string_lossy().trim().is_empty() {
            return Err(VideoExportError::InvalidConfig(
                tr!("请先选择视频输出路径").to_string(),
            ));
        }
        let ffmpeg = ffmpeg_path()?;

        tracing::info!(path = %ffmpeg.display(), "启动 ffmpeg 编码器（直连写入模式）");

        let args = build_ffmpeg_args(config, input_pix_fmt);
        tracing::debug!(?args, "ffmpeg 参数");

        let mut process = Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = match process.stdin.take() {
            Some(stdin) => stdin,
            None => {
                terminate_child(&mut process);
                return Err(VideoExportError::PipeSetupFailed(
                    tr!("ffmpeg stdin 未 piped").into(),
                ));
            }
        };
        let stderr = match process.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate_child(&mut process);
                return Err(VideoExportError::PipeSetupFailed(
                    tr!("ffmpeg stderr 未 piped").into(),
                ));
            }
        };

        let stderr_buf: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let writer_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let stderr_buf_clone = Arc::clone(&stderr_buf);
        let stderr_thread = match thread::Builder::new()
            .name("ffmpeg-stderr-video".into())
            .spawn(move || {
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
                                    push_stderr_tail(&mut buf, trimmed);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[ffmpeg] stderr 读取错误: {}", e);
                            break;
                        }
                    }
                }
            }) {
            Ok(handle) => handle,
            Err(e) => {
                terminate_child(&mut process);
                return Err(VideoExportError::PipeSetupFailed(trf!(
                    "无法启动 ffmpeg stderr 线程: {e}",
                    e = e
                )));
            }
        };

        let stdin_writer = Some(BufWriter::with_capacity(1024 * 1024, stdin));

        Ok(Self {
            stdin_writer,
            process,
            width: config.width,
            height: config.height,
            stderr_buf,
            stderr_thread: Some(stderr_thread),
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
            return Err(self.build_write_error(tr!("stdin_writer 已关闭，无法写入帧")));
        };
        if let Err(e) = writer.write_all(&frame_data) {
            let msg = trf!("write_all 失败: {e}", e = e);
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

        // 有界等待；超时路径会终止并回收子进程，随后 Drop 回收 stderr 线程。
        let status = wait_for_process(&mut self.process, FFMPEG_FINISH_TIMEOUT).map_err(|e| {
            VideoExportError::FfmpegWriteFailed(format!(
                "等待 ffmpeg 完成失败: {e}\nffmpeg stderr:\n{}",
                self.stderr_content()
            ))
        })?;
        self.join_stderr_thread();
        if !status.success() {
            let stderr_lines = self.stderr_content();
            tracing::error!(
                code = status.code(),
                stderr = %stderr_lines,
                "ffmpeg 非零退出"
            );
            let msg = match status.code() {
                Some(code) => trf!("ffmpeg 退出码 {code}", code = code),
                None => tr!("ffmpeg 未知错误退出").to_string(),
            };
            return Err(VideoExportError::FfmpegWriteFailed(format!(
                "{msg}\nffmpeg stderr:\n{stderr_lines}"
            )));
        }

        tracing::info!("ffmpeg 编码完成");
        Ok(())
    }

    fn join_stderr_thread(&mut self) {
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }

    /// 拼接捕获的 stderr 尾部行
    fn stderr_content(&self) -> String {
        self.stderr_buf
            .lock()
            .map(|buf| {
                buf.iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    /// 构建带 writer 错误与 stderr 上下文的 FfmpegWriteFailed
    fn build_write_error(&self, context: &str) -> VideoExportError {
        let mut parts = vec![context.to_string()];
        if let Ok(writer_err) = self.writer_error.lock()
            && let Some(ref e) = *writer_err
        {
            parts.push(trf!("写入错误: {e}", e = e));
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
        // 用户取消时 kill ffmpeg 进程；EOF 后 stderr 线程退出并被回收。
        terminate_child(&mut self.process);
        self.join_stderr_thread();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::process::Child;
    use std::time::Duration;

    /// 空输出路径必须在**启动 ffmpeg 之前**被拒绝（用户实测 `code=-22` 根因）。
    ///
    /// 校验先于 `ffmpeg_path()`：本测试不依赖本机是否安装 ffmpeg。
    #[test]
    fn test_new_rejects_blank_output_path_before_spawning() {
        use crate::video::config::{Container, EncoderBackend, QualityPreset, VideoCodec};

        let config = VideoExportConfig {
            width: 1920,
            height: 1080,
            fps: 60.0,
            container: Container::Mp4,
            codec: VideoCodec::H264,
            backend: EncoderBackend::Software,
            output_path: std::path::PathBuf::new(),
            quality: QualityPreset::Medium,
        };
        let err =
            FfmpegEncoder::new(&config, "bgra").expect_err("空输出路径必须在启动 ffmpeg 前被拒绝");
        assert!(
            matches!(err, VideoExportError::InvalidConfig(_)),
            "错误类型必须是 InvalidConfig，实际 {err:?}"
        );
        assert!(
            err.to_string().contains("请先选择视频输出路径"),
            "错误必须可直接展示给用户，实际 {err}"
        );
    }

    #[test]
    fn test_stderr_tail_keeps_only_latest_lines() {
        let mut tail = VecDeque::new();
        for index in 0..201 {
            push_stderr_tail(&mut tail, format!("line-{index}"));
        }

        assert_eq!(tail.len(), 200);
        assert_eq!(tail.front().map(String::as_str), Some("line-1"));
        assert_eq!(tail.back().map(String::as_str), Some("line-200"));
    }

    #[test]
    fn test_wait_for_process_timeout_helper() {
        if std::env::var_os("LUMINO_FFMPEG_WAIT_TEST_SLEEP").is_some() {
            thread::sleep(Duration::from_secs(5));
        }
    }

    fn spawn_test_child(sleep: bool) -> Child {
        let mut command = Command::new(std::env::current_exe().expect("应取得测试可执行文件"));
        command.args([
            "--exact",
            "video::ffmpeg::tests::test_wait_for_process_timeout_helper",
            "--nocapture",
        ]);
        if sleep {
            command.env("LUMINO_FFMPEG_WAIT_TEST_SLEEP", "1");
        }
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动测试子进程应成功")
    }

    #[test]
    fn test_wait_for_process_returns_after_normal_exit() {
        let mut child = spawn_test_child(false);
        let status = wait_for_process(&mut child, Duration::from_secs(5))
            .expect("正常退出的子进程应完成等待");
        assert!(status.success());
    }

    #[test]
    fn test_wait_for_process_kills_timed_out_child() {
        let mut child = spawn_test_child(true);
        let result = wait_for_process(&mut child, Duration::from_millis(50));

        assert!(result.is_err(), "挂起子进程必须报告超时");
        assert_eq!(
            result.expect_err("等待必须超时").kind(),
            std::io::ErrorKind::TimedOut
        );
        assert!(
            child.try_wait().expect("查询子进程状态应成功").is_some(),
            "超时后必须终止并回收子进程"
        );
    }
}
