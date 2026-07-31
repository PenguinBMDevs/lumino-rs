//! 实时合成器核心实现
//!
//! 高性能设计：自包含的渲染线程 + 锁无关音频回调路径。
//!
//! - 渲染线程独自拥有 `ChannelGroup`，从 bounded channel 消费 MIDI 事件，
//!   按固定窗口大小渲染音频样本，通过 channel 发送给音频回调。
//! - 音频回调仅做 lock-free 的 `try_recv` + limiter + 样本转换，
//!   零锁、零分配、零线程池开销。
//! - 相比 xsynth-realtime 的架构，移除了 `Mutex<BufferedRenderer>` 包装层
//!   和 rayon 在音频回调中的并行开销，显著降低延迟与卡顿风险。

mod audio_stream;
mod event;
mod open;
mod render;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::StreamTrait;
use cpal::Stream;
use crossbeam_channel::Sender;

use xsynth_core::AudioStreamParams;

use crate::events::SynthEvent;
use crate::stats::{RealtimeSynthStats, RealtimeSynthStatsReader, RenderPerfShared, RenderPerfStats};

/// 发送同步/同步的流包装器
struct SendSyncStream(Stream);
unsafe impl Sync for SendSyncStream {}
unsafe impl Send for SendSyncStream {}

/// 实时合成器
pub struct RealtimeSynth {
    /// 事件发送器
    sender: Sender<SynthEvent>,
    /// 音频流
    stream: Option<SendSyncStream>,
    /// 统计信息
    stats: RealtimeSynthStats,
    /// 性能计数器
    perf: Arc<RenderPerfShared>,
    /// 流参数
    stream_params: AudioStreamParams,
    /// 渲染线程句柄
    render_thread: Option<std::thread::JoinHandle<()>>,
    /// 渲染线程运行标志
    running: Arc<AtomicBool>,
}

impl RealtimeSynth {
    /// 发送合成器事件
    pub fn send_event(&mut self, event: SynthEvent) {
        let _ = self.sender.send(event);
    }

    /// 获取事件发送器引用
    pub fn get_sender_ref(&self) -> Option<&crossbeam_channel::Sender<SynthEvent>> {
        Some(&self.sender)
    }

    /// 获取统计信息快照
    pub fn get_stats(&self) -> RealtimeSynthStatsReader {
        RealtimeSynthStatsReader {
            voice_count: self.stats.voice_count.load(Ordering::Relaxed),
            average_renderer_load: f64::from_bits(self.perf.average_load.load(Ordering::Relaxed)),
            last_samples_after_read: 0,
        }
    }

    /// 获取性能统计
    pub fn perf_stats(&self) -> RenderPerfStats {
        self.perf.snapshot()
    }

    /// 获取流参数
    pub fn stream_params(&self) -> AudioStreamParams {
        self.stream_params
    }

    /// 获取通道数
    pub fn channel_count(&self) -> u32 {
        self.stream_params.channels.count() as u32
    }

    /// 暂停音频输出
    pub fn pause(&mut self) -> Result<(), cpal::PauseStreamError> {
        if let Some(stream) = &mut self.stream {
            stream.0.pause()
        } else {
            Ok(())
        }
    }

    /// 恢复音频输出
    pub fn resume(&mut self) -> Result<(), cpal::PlayStreamError> {
        if let Some(stream) = &mut self.stream {
            stream.0.play()
        } else {
            Ok(())
        }
    }
}

impl Drop for RealtimeSynth {
    fn drop(&mut self) {
        // 1) 信号量：通知渲染线程退出
        self.running.store(false, Ordering::Relaxed);
        // 2) 释放音频流：sample_rx 析构 → send() 失败 → 渲染线程也退出
        self.stream.take();
        // 3) 等待渲染线程终止
        self.render_thread.take();
    }
}
