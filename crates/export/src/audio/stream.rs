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
    pub fn new() -> Self {
        VecSampleSink {
            samples: Vec::new(),
        }
    }

    /// 消费自己，返回收集的样本
    pub fn into_samples(mut self) -> Vec<f32> {
        std::mem::take(&mut self.samples)
    }

    /// 返回当前样本数
    pub fn len(&self) -> usize {
        self.samples.len()
    }

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

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

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
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args([
            "-y",
            "-f",
            "f32le",
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &channels.to_string(),
            "-i",
            "pipe:0",
        ]);

        if bitrate > 0 && codec.has_bitrate() {
            cmd.args(["-b:a", &format!("{}k", bitrate)]);
        }

        let output_str = output_path.to_str().ok_or_else(|| {
            ExportError::AudioWrite(format!("输出路径不是合法 UTF-8: {}", output_path.display()))
        })?;
        cmd.args(["-c:a", codec_name, output_str]);

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

        Ok(FfmpegSink {
            process: Some(process),
            stdin: Some(stdin),
        })
    }

    /// 等待 ffmpeg 进程完成并检查错误
    fn wait_for_completion(&mut self) -> ExportResult<()> {
        if let Some(mut process) = self.process.take() {
            let status = process
                .wait()
                .map_err(|e| ExportError::AudioWrite(format!("ffmpeg 进程等待失败: {e}")))?;

            if !status.success() {
                // 读取 stderr 获取错误信息
                if let Some(stderr) = process.stderr.take() {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::BufReader::new(stderr)
                        .read_to_string(&mut buf)
                        .ok();
                    if !buf.is_empty() {
                        return Err(ExportError::AudioWrite(format!(
                            "ffmpeg 编码失败:\n{}",
                            buf
                        )));
                    }
                }
                return Err(ExportError::AudioWrite(format!(
                    "ffmpeg 进程退出码: {:?}",
                    status.code()
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
