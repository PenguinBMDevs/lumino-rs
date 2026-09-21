//! Realtime playback of a [`crate::GpuSynth`] through `cpal`.
//!
//! # Architecture
//!
//! The engine is owned by a background **render thread** that renders
//! continuously, paced by the block's realtime budget (90% of wall-clock
//! duration). MIDI events are sent through a channel and drained before
//! each block. Rendered blocks are pushed to the audio callback through a
//! bounded queue; when the render thread is ahead of the consumer by more
//! than 10% it sleeps, otherwise it keeps rendering — it **never drops a
//! block**. The audio callback runs on the OS audio thread and must never
//! block: it only copies from the queue and writes silence on underrun
//! (counting them in the stats).
//!
//! # Sample-rate negotiation
//!
//! The engine renders at its configured sample rate (e.g. 64 kHz). Most
//! output devices do not run at 64 kHz, so the playback layer picks the
//! device's default configuration first and falls back to any supported
//! config whose sample rate matches the engine; if none matches, it
//! resamples the engine output to the device rate with a small linear
//! interpolator. Use [`AudioPlayback::device_sample_rates`] to list what
//! a device supports before constructing the engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// DIAG: throttle for the `[UNDERRUN]` stderr marker below.
static LAST_UD_LOG: AtomicI64 = AtomicI64::new(0);

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::resample::SincResampler;
use crate::GpuSynth;
use crate::SynthError;
use crate::midi::MidiEvent;

mod api;
mod config;
mod start;
mod stats;

/// Read-only view of the realtime playback statistics.
///
/// Mirrors the stats exposed by XSynth's `BufferedRenderer` so a status
/// line like "Voice Count / Buffer / Render time" can be printed while
/// playing.
///
/// All counters are lock-free (atomics + a fixed ring of `AtomicU64` slots):
/// the render thread must never block on a lock, so stats are published
/// with relaxed stores and readers take a best-effort snapshot.
#[derive(Clone)]
pub struct PlaybackStatsReader {
    samples: Arc<AtomicI64>,
    last_request_samples: Arc<AtomicI64>,
    last_samples_after_read: Arc<AtomicI64>,
    /// Ring of recent render-load percentages (0..n), stored as f64 bits in
    /// `AtomicU64`. `render_time_head` is the next slot to write.
    render_time: Arc<[AtomicU64; STATS_RING]>,
    render_time_head: Arc<AtomicU64>,
    render_size: Arc<AtomicU64>,
    voice_count: Arc<AtomicU64>,
    underruns: Arc<AtomicU64>,
}

/// Number of recent render-load samples kept for the moving average.
const STATS_RING: usize = 128;

/// A running realtime playback session.
///
/// # Example
///
/// ```no_run
/// use lumino_gpu_synth::{GpuSynth, SynthConfig};
/// use lumino_gpu_synth::audio::playback::AudioPlayback;
///
/// let mut synth = GpuSynth::new(SynthConfig::default())?;
/// synth.load_soundfont("assets/test.sf2", 0, 0)?;
/// let mut playback = AudioPlayback::start(synth, None)?;
/// playback.note_on(0, 60, 100);
/// std::thread::sleep(std::time::Duration::from_millis(500));
/// playback.note_off(0, 60);
/// playback.stop();
/// # Ok::<(), lumino_gpu_synth::SynthError>(())
/// ```
pub struct AudioPlayback {
    stop_flag: Arc<AtomicBool>,
    stop_tx: Option<mpsc::Sender<()>>,
    event_tx: Option<mpsc::Sender<(u8, MidiEvent)>>,
    stream_tx: Option<mpsc::Sender<Vec<crate::midi::TimedEvent>>>,
    thread: Option<JoinHandle<()>>,
    sample_rate: u32,
    engine_rate: u32,
    stats: PlaybackStatsReader,
    /// 持有「stream owner」线程的句柄。
    ///
    /// 与 xsynth-realtime 相同：cpal 0.15 的 `Stream` 在 Windows 上是 `!Send`
    /// （携带 `NotSendSyncAcrossAllPlatforms`），不能在 `Send` 结构上跨线程移动。
    /// 因此 `Stream` 在 stream owner 线程内部创建并存活，这里只持有其 `JoinHandle`
    /// （`Send + Sync`），从而让整个后端满足 `Api: Send + Sync`。
    _stream_owner: Option<JoinHandle<()>>,
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        self.stop();
    }
}
