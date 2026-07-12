//! 音频文件写入器 — 基于 hound 的 WAV 输出
//!
//! 支持 Vec 回收：写入线程消费完样本后通过 `vec_return` 通道将 Vec 归还，
//! 渲染线程可复用这些 Vec，减少重复分配。

use std::{
    path::Path,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use hound::{WavSpec, WavWriter};

use crate::error::{ExportError, ExportResult};

/// 通过独立线程写入 WAV 文件的异步写入器
///
/// 支持 Vec 回收：写入线程完成写入后通过 `vec_return_tx` 将 Vec 归还给渲染线程。
pub struct AudioFileWriter {
    /// Option 包装以便 finalize 时 take 丢弃 sender 关闭通道
    sender: Option<Sender<Vec<f32>>>,
    handle: Option<JoinHandle<Result<(), hound::Error>>>,
}

impl AudioFileWriter {
    /// 创建新的 WAV 写入器。
    ///
    /// `vec_return_tx` 是 Vec 回收通道的发送端 — 写入线程写完后将 Vec 发回，
    /// 渲染线程通过对应的 `Receiver` 回收。
    pub fn new(
        sample_rate: u32,
        channels: u16,
        path: &Path,
        vec_return_tx: Sender<Vec<f32>>,
    ) -> ExportResult<Self> {
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
                    // 逐个样点写入
                    for s in &batch {
                        if let Err(e) = w.write_sample(*s) {
                            tracing::error!("WAV 写入错误: {e}");
                            // 即使写入失败也继续回收 Vec
                            let _ = vec_return_tx.send(batch);
                            return Err(e);
                        }
                    }
                    // 写入完成后将 Vec 归还给渲染线程复用
                    let _ = vec_return_tx.send(batch);
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

    /// 将一批样点发送到写入线程。
    ///
    /// 通过 `take` 转移所有权，避免拷贝。
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
