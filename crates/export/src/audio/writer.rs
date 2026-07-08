//! 音频文件写入器 — 基于 hound 的 WAV 输出

use std::{
    path::Path,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use hound::{WavSpec, WavWriter};

use crate::error::{ExportError, ExportResult};

/// 通过独立线程写入 WAV 文件的异步写入器
pub struct AudioFileWriter {
    /// Option 包装以便 finalize 时 take 丢弃 sender 关闭通道
    sender: Option<Sender<Vec<f32>>>,
    handle: Option<JoinHandle<Result<(), hound::Error>>>,
}

impl AudioFileWriter {
    /// 创建新的 WAV 写入器
    pub fn new(sample_rate: u32, channels: u16, path: &Path) -> ExportResult<Self> {
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let writer = WavWriter::create(path, spec)
            .map_err(|e| ExportError::AudioWrite(format!("无法创建 WAV 文件: {e}")))?;

        let (snd, rcv): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = unbounded();

        let handle = thread::Builder::new()
            .name("audio-writer".into())
            .spawn(move || {
                let mut w = writer;
                for batch in rcv {
                    for s in batch {
                        w.write_sample(s)
                            .map_err(|e| {
                                tracing::error!("WAV 写入错误: {e}");
                                e
                            })?;
                    }
                }
                w.finalize().map_err(|e| {
                    tracing::error!("WAV finalize 错误: {e}");
                    e
                })?;
                tracing::debug!("音频写入线程完成");
                Ok(())
            })
            .map_err(|e| ExportError::AudioWrite(format!("无法创建写入线程: {e}")))?;

        Ok(Self {
            sender: Some(snd),
            handle: Some(handle),
        })
    }

    /// 将一批样点发送到写入线程
    pub fn write_samples(&mut self, samples: &mut Vec<f32>) -> ExportResult<()> {
        let buf = std::mem::take(samples);
        self.sender
            .as_ref()
            .ok_or_else(|| ExportError::AudioWrite("写入通道已关闭".into()))?
            .send(buf)
            .map_err(|_| ExportError::AudioWrite("写入通道已关闭".into()))
    }

    /// 关闭写入器，等待写入线程完成
    pub fn finalize(mut self) -> ExportResult<()> {
        // 丢弃 sender → 通道关闭 → 写入线程 for 循环结束 → finalize → join
        drop(self.sender.take());

        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| ExportError::AudioWrite("写入线程异常退出".into()))?
                .map_err(|e| ExportError::AudioWrite(format!("WAV 写入失败: {e}")))?;
        }
        Ok(())
    }
}

impl Drop for AudioFileWriter {
    fn drop(&mut self) {
        if self.sender.is_none() {
            return; // 已 finalize
        }
        tracing::warn!("AudioFileWriter 未调用 finalize() 就被丢弃");
    }
}
