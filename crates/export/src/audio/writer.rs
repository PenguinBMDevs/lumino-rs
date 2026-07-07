//! 音频文件写入器——WAV/FLAC 输出

use std::path::Path;

use crate::error::{ExportError, ExportResult};

use super::types::{AudioChannels, AudioFormat};

/// 音频文件写入器
pub enum AudioFileWriter {
    WAV(hound::WavWriter<std::io::BufWriter<std::fs::File>>),
    FLAC {
        path: std::path::PathBuf,
        sample_rate: u32,
        channels: u16,
        samples: Vec<i16>,
    },
}

impl AudioFileWriter {
    /// 创建新的音频文件写入器
    pub fn create(
        path: &Path,
        format: AudioFormat,
        sample_rate: u32,
        channels: AudioChannels,
    ) -> ExportResult<Self> {
        match format {
            AudioFormat::WAV => {
                let spec = hound::WavSpec {
                    channels: channels.count(),
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let writer = hound::WavWriter::create(path, spec)
                    .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                Ok(Self::WAV(writer))
            }
            AudioFormat::FLAC => {
                // FLAC: 收集所有样本，最后一次性编码
                Ok(Self::FLAC {
                    path: path.to_path_buf(),
                    sample_rate,
                    channels: channels.count(),
                    samples: Vec::new(),
                })
            }
        }
    }

    /// 写入样本
    ///
    /// 流式转换 f32 → i16：跳过中间 `Vec<i16>` 分配，直接流式写入或追加。
    pub fn write_samples(&mut self, buf: &[f32]) -> ExportResult<()> {
        match self {
            Self::WAV(writer) => {
                for &s in buf {
                    let clamped = s.clamp(-1.0, 1.0);
                    writer
                        .write_sample((clamped * 32767.0) as i16)
                        .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                }
                Ok(())
            }
            Self::FLAC {
                samples: flac_buf, ..
            } => {
                // 直接流式 push，跳过中间 Vec 分配
                flac_buf.reserve(buf.len());
                for &s in buf {
                    let clamped = s.clamp(-1.0, 1.0);
                    flac_buf.push((clamped * 32767.0) as i16);
                }
                Ok(())
            }
        }
    }

    /// 完成写入
    pub fn finalize(self) -> ExportResult<()> {
        match self {
            Self::WAV(writer) => {
                writer
                    .finalize()
                    .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                Ok(())
            }
            Self::FLAC {
                path,
                sample_rate,
                channels,
                samples,
            } => {
                // 将 i16 转换为 f32 (范围 -1.0 到 1.0)
                let f32_samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32767.0).collect();

                // 使用 flac-encoder 编码
                let flac_data = flac_encoder::FlacBuilder::from_interleaved(
                    &f32_samples,
                    channels as usize,
                    sample_rate,
                )
                .build()
                .map_err(|e| ExportError::AudioWrite(format!("FLAC 编码失败: {:?}", e)))?;

                std::fs::write(&path, &flac_data)
                    .map_err(|e| ExportError::AudioWrite(format!("FLAC 文件写入失败: {}", e)))?;

                Ok(())
            }
        }
    }
}
