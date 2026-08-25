//! Core 后端占位实现（3/4 将替换为真实 ChannelGroup+ring）
//!
//! 此处先提供最小可用的 `OutputConnection` 实现，保证 `BackendKind::Core` 可创建、
//! 可切换、混音台可读写增益/峰值，音频线程零分配/零锁的完整实现见下一步。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::{Error, OutputConnection};

/// 用于混音台电平回读的共享峰值（与 Realtime 的 ChannelMix 语义对齐）
#[derive(Debug)]
struct LevelStore {
    channel_levels: [AtomicU32; 16],
    master: AtomicU32,
    gains: [AtomicU32; 16],
    pans: [AtomicU32; 16],
}

impl LevelStore {
    fn new() -> Self {
        Self {
            channel_levels: std::array::from_fn(|_| AtomicU32::new(0)),
            master: AtomicU32::new(0),
            gains: std::array::from_fn(|_| AtomicU32::new(f32::to_bits(1.0))),
            pans: std::array::from_fn(|_| AtomicU32::new(f32::to_bits(0.0))),
        }
    }
}

/// Core 后端输出连接（占位）
///
/// 3/4 将替换内部为：`AudioRingProducer + cpal Stream + Renderer Thread + Worker Thread + ChannelGroup`
pub struct CoreOutput {
    _soundfont_path: PathBuf,
    _sample_rate: u32,
    levels: Arc<LevelStore>,
}

impl CoreOutput {
    pub fn new(soundfont_path: PathBuf, sample_rate: Option<u32>) -> Result<Self, Error> {
        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont not found: {:?}",
                soundfont_path
            )));
        }
        let sr = sample_rate.unwrap_or(crate::constants::DEFAULT_SAMPLE_RATE);
        Ok(Self {
            _soundfont_path: soundfont_path,
            _sample_rate: sr,
            levels: Arc::new(LevelStore::new()),
        })
    }
}

impl OutputConnection for CoreOutput {
    fn send_raw(&mut self, _data: [u8; 3]) -> Result<(), Error> {
        // 占位：3/4 在 renderer 线程中统一走 ChannelGroup dispatch
        Ok(())
    }

    fn set_channel_gain(&mut self, ch: u8, gain: f32) -> Result<(), Error> {
        if let Some(slot) = self.levels.gains.get(ch as usize) {
            slot.store(gain.max(0.0).to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    fn set_channel_pan(&mut self, ch: u8, pan: f32) -> Result<(), Error> {
        if let Some(slot) = self.levels.pans.get(ch as usize) {
            slot.store(pan.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    fn get_channel_levels(&self) -> [f32; 16] {
        let mut out = [0.0f32; 16];
        for (i, a) in self.levels.channel_levels.iter().enumerate() {
            out[i] = f32::from_bits(a.load(Ordering::Relaxed));
        }
        out
    }

    fn get_master_level(&self) -> f32 {
        f32::from_bits(self.levels.master.load(Ordering::Relaxed))
    }

    fn close(self: Box<Self>) {}
}
