//! 音频流写入器 — 参考 OmniConverter 的 MultiStreamMerger / ISampleWriter
//!
//! 提供两种写入模式：
//! - [`SampleSink`]：直接写入 f32 样本到 Vec，支持多写入器合并（类似 MultiStreamMerger）
//! - [`WavFileSink`]：通过 hound 写入 WAV 文件
//! - [`FfmpegSink`]：通过 FFmpeg 子进程编码为非 WAV 格式

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use crate::error::{ExportError, ExportResult};

use super::codec::AudioCodec;

/// 样本接收器 trait — 定义写入音频样本的接口
pub trait SampleSink: Send {
    /// 写入一批 PCM f32 样本（interleaved）
    fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()>;

    /// 跳过（填充零）指定数量的样本
    fn skip_samples(&mut self, count: usize) -> ExportResult<()>;

    /// 完成写入，刷新缓冲区
    fn finalize(&mut self) -> ExportResult<()>;
}

/// 内存样本接收器 — 将样本收集到 Vec 中，支持后续合并或回读
pub struct VecSampleSink {
    samples: Vec<f32>,
}

impl VecSampleSink {
    /// 创建一个空的内存样本接收器
    pub fn new() -> Self {
        VecSampleSink {
            samples: Vec::new(),
        }
    }
}

impl Default for VecSampleSink {
    fn default() -> Self {
        Self::new()
    }
}

impl VecSampleSink {
    /// 消费自己，返回收集的样本
    pub fn into_samples(mut self) -> Vec<f32> {
        std::mem::take(&mut self.samples)
    }

    /// 返回当前样本数
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// 返回当前是否没有样本
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

impl SampleSink for VecSampleSink {
    fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()> {
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn skip_samples(&mut self, count: usize) -> ExportResult<()> {
        self.samples.resize(self.samples.len() + count, 0.0);
        Ok(())
    }

    fn finalize(&mut self) -> ExportResult<()> {
        Ok(())
    }
}

/// WAV 文件写入器 — 通过 hound 写入 32-bit float WAV
pub struct WavFileSink {
    writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
    sample_rate: u32,
    channels: u16,
}

impl WavFileSink {
    /// 创建 WAV 写入器（32-bit float 格式）
    pub fn new(path: &Path, sample_rate: u32, channels: u16) -> ExportResult<Self> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let file = std::fs::File::create(path)
            .map_err(|e| ExportError::AudioWrite(format!("无法创建 WAV 文件 {path:?}: {e}")))?;
        let writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)
            .map_err(|e| ExportError::AudioWrite(format!("WAV 写入器初始化失败: {e}")))?;

        Ok(WavFileSink {
            writer: Some(writer),
            sample_rate,
            channels,
        })
    }

    /// 返回采样率
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 返回声道数
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl SampleSink for WavFileSink {
    fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| ExportError::AudioWrite("WAV 写入器已关闭".into()))?;

        for &s in samples {
            writer
                .write_sample(s)
                .map_err(|e| ExportError::AudioWrite(format!("WAV 写入错误: {e}")))?;
        }
        Ok(())
    }

    fn skip_samples(&mut self, count: usize) -> ExportResult<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| ExportError::AudioWrite("WAV 写入器已关闭".into()))?;

        for _ in 0..count {
            writer
                .write_sample(0.0f32)
                .map_err(|e| ExportError::AudioWrite(format!("WAV 写入错误: {e}")))?;
        }
        Ok(())
    }

    fn finalize(&mut self) -> ExportResult<()> {
        if let Some(writer) = self.writer.take() {
            writer
                .finalize()
                .map_err(|e| ExportError::AudioWrite(format!("WAV finalize 失败: {e}")))?;
        }
        Ok(())
    }
}

impl Drop for WavFileSink {
    fn drop(&mut self) {
        if self.writer.is_some() {
            tracing::warn!("WavFileSink 未调用 finalize() 就被丢弃");
        }
    }
}

/// FFmpeg 音频写入器 — 通过 ffmpeg 子进程编码为非 WAV 格式
pub struct FfmpegSink {
    process: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    /// ffmpeg stderr 尾部（排空线程实时写入；失败时用于错误信息）
    stderr_tail: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// stderr 排空线程（DEBT-07 #124：必须实时读走，否则管道写满 → `wait()` 死锁）
    stderr_thread: Option<std::thread::JoinHandle<()>>,
}

/// 构建 ffmpeg 编码参数（纯函数，供单测断言参数口径）。
fn build_ffmpeg_args(
    codec_name: &str,
    sample_rate: u32,
    channels: u16,
    bitrate: u32,
    has_bitrate: bool,
    output_str: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        // DEBT-07 #124：禁用 ffmpeg 进度统计输出——配合 stderr 排空线程双保险，
        // 防止管道缓冲区写满导致的经典 wait 死锁（旧实现只在 wait 后才读 stderr）。
        "-nostats".into(),
        "-f".into(),
        "f32le".into(),
        "-ar".into(),
        sample_rate.to_string(),
        "-ac".into(),
        channels.to_string(),
        "-i".into(),
        "pipe:0".into(),
    ];

    if bitrate > 0 && has_bitrate {
        args.push("-b:a".into());
        args.push(format!("{bitrate}k"));
    }

    args.push("-c:a".into());
    args.push(codec_name.into());
    args.push(output_str.into());
    args
}

impl FfmpegSink {
    /// 创建 FFmpeg 编码器进程
    ///
    /// # 参数
    /// - `ffmpeg_path`: ffmpeg 可执行文件路径
    /// - `output_path`: 输出文件路径
    /// - `codec`: 目标编码器
    /// - `sample_rate`: 采样率
    /// - `channels`: 声道数
    /// - `bitrate`: 比特率（部分编码器使用）
    pub fn new(
        ffmpeg_path: &Path,
        output_path: &Path,
        codec: AudioCodec,
        sample_rate: u32,
        channels: u16,
        bitrate: u32,
    ) -> ExportResult<Self> {
        let codec_name = codec
            .ffmpeg_codec()
            .ok_or_else(|| ExportError::AudioWrite("PCM 不需要 ffmpeg 编码".into()))?;

        // 使用 stdin pipe (pipe:0) 向 ffmpeg 输入 PCM 数据
        let output_str = output_path.to_str().ok_or_else(|| {
            ExportError::AudioWrite(format!("输出路径不是合法 UTF-8: {}", output_path.display()))
        })?;
        let args = build_ffmpeg_args(
            codec_name,
            sample_rate,
            channels,
            bitrate,
            codec.has_bitrate(),
            output_str,
        );
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(&args);

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut process = cmd
            .spawn()
            .map_err(|e| ExportError::AudioWrite(format!("无法启动 ffmpeg: {e}")))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| ExportError::AudioWrite("无法获取 ffmpeg stdin".into()))?;

        // DEBT-07 #124：实时排空 stderr。旧实现把 stderr 设为 piped 却只在
        // `wait()` 之后才读——ffmpeg 持续写进度统计，管道缓冲（Windows 约 4-64KB）
        // 写满后子进程阻塞、父进程 wait 永久挂起（长导出必现）。
        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| ExportError::AudioWrite("无法获取 ffmpeg stderr".into()))?;
        let stderr_tail: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tail_clone = std::sync::Arc::clone(&stderr_tail);
        let stderr_thread = std::thread::Builder::new()
            .name("ffmpeg-stderr-audio".into())
            .spawn(move || {
                use std::io::BufRead;
                const MAX_TAIL_LINES: usize = 200;
                let mut reader = std::io::BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if let Ok(mut buf) = tail_clone.lock() {
                                if buf.len() >= MAX_TAIL_LINES {
                                    buf.remove(0);
                                }
                                buf.push(trimmed.to_string());
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| ExportError::AudioWrite(format!("无法启动 stderr 排空线程: {e}")))?;

        Ok(FfmpegSink {
            process: Some(process),
            stdin: Some(stdin),
            stderr_tail,
            stderr_thread: Some(stderr_thread),
        })
    }

    /// 等待 ffmpeg 进程完成并检查错误
    fn wait_for_completion(&mut self) -> ExportResult<()> {
        if let Some(mut process) = self.process.take() {
            let status = process
                .wait()
                .map_err(|e| ExportError::AudioWrite(format!("ffmpeg 进程等待失败: {e}")))?;

            // 进程退出 → stderr EOF → 排空线程收尾；join 防线程泄漏
            if let Some(handle) = self.stderr_thread.take() {
                let _ = handle.join();
            }
            let stderr_text = {
                let tail = self.stderr_tail.lock().unwrap_or_else(|e| e.into_inner());
                tail.join("\n")
            };

            if !status.success() {
                if stderr_text.is_empty() {
                    return Err(ExportError::AudioWrite(format!(
                        "ffmpeg 进程退出码: {:?}",
                        status.code()
                    )));
                }
                return Err(ExportError::AudioWrite(format!(
                    "ffmpeg 编码失败:\n{stderr_text}"
                )));
            }
        }
        Ok(())
    }
}

impl SampleSink for FfmpegSink {
    fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| ExportError::AudioWrite("ffmpeg 管道已关闭".into()))?;

        // 将 f32 样本转换为 little-endian 字节写入
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        stdin
            .write_all(&bytes)
            .map_err(|e| ExportError::AudioWrite(format!("ffmpeg 管道写入错误: {e}")))?;

        Ok(())
    }

    fn skip_samples(&mut self, count: usize) -> ExportResult<()> {
        // 跳过样本 = 写入零（静音）
        let zeros = vec![0.0f32; count];
        self.write_samples(&zeros)
    }

    fn finalize(&mut self) -> ExportResult<()> {
        // 关闭 stdin，等待 ffmpeg 完成
        if let Some(stdin) = self.stdin.take() {
            drop(stdin);
        }
        self.wait_for_completion()
    }
}

impl Drop for FfmpegSink {
    fn drop(&mut self) {
        // 如果进程还在运行，强制终止
        if self.stdin.is_some() {
            let _ = self.stdin.take();
        }
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        // 进程死后 stderr 到达 EOF，排空线程自然收尾；join 防线程泄漏
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }
}

impl SampleSink for Box<dyn SampleSink> {
    fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()> {
        (**self).write_samples(samples)
    }

    fn skip_samples(&mut self, count: usize) -> ExportResult<()> {
        (**self).skip_samples(count)
    }

    fn finalize(&mut self) -> ExportResult<()> {
        (**self).finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEBT-07 #124：编码参数必须含 `-nostats`（配合 stderr 排空防死锁）。
    #[test]
    fn ffmpeg_args_include_nostats_and_codec() {
        let args = build_ffmpeg_args("libmp3lame", 48000, 2, 320, true, "out.mp3");
        assert!(
            args.iter().any(|a| a == "-nostats"),
            "必须禁用 ffmpeg 进度统计: {args:?}"
        );
        let codec_pos = args
            .iter()
            .position(|a| a == "-c:a")
            .expect("-c:a 必须存在");
        assert_eq!(args[codec_pos + 1], "libmp3lame");
        assert!(args.iter().any(|a| a == "320k"), "有比特率能力时应带 -b:a");
        assert_eq!(args.last().map(String::as_str), Some("out.mp3"));
    }

    /// 无比特率能力的编码器不得携带 `-b:a`（零回归口径）。
    #[test]
    fn ffmpeg_args_omit_bitrate_when_unsupported() {
        let args = build_ffmpeg_args("flac", 44100, 1, 320, false, "out.flac");
        assert!(!args.iter().any(|a| a == "-b:a"), "FLAC 不应带 -b:a");
    }
}
