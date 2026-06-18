//! 洋葱皮计算 coordinator — 基于 Kiva 分桶 + 游标复用
//!
//! == 架构 ==
//! `NoteWorker` 是 coordinator 线程，接收 `OnionSkinJob` 后处理。
//! 数据来自 `OnionSkinBucket`（按 key 分桶、按 start_tick 升序），
//! Worker 持有渲染游标，正向滚动时只扫描新进入视口的音符。
//!
//! == 数据流 ==
//!   Main Thread              NoteWorker (coordinator)          WGPU Thread
//!     │                            │                               │
//!     ├─ Arc<OnionSkinBucket> ────►├─ 按 key 游标扫描             │
//!     │   + 视口参数               │  ├─ 可见性过滤                │
//!     │                            │  └─ sort visible subset      │
//!     │                            ├─ swap combined buffer ───────►│
//!     │                            │                               ├─ upload to GPU

use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use lumino_gfx::{OnionCollectParams, OnionNote, OnionSkinBucket, SwappableBuffer};

// ─── 数据快照 ───────────────────────────────────────────────────────────────

/// 洋葱皮计算所需的全部数据快照（Send 安全）
pub(crate) struct OnionSkinComputationSnapshot {
    pub visible_tick_start: f32,
    pub visible_tick_end: f32,
    pub visible_key_min: u16,
    pub visible_key_max: u16,
    pub onion_skin_enabled: bool,
    pub track_onion_states: HashMap<usize, bool>,
    pub current_track: usize,
    /// 按 key 分桶的洋葱皮数据（跨线程共享）
    pub onion_bucket: Option<Arc<OnionSkinBucket>>,
    /// bucket 版本号，用于 Worker 检测数据变化并重置游标
    pub bucket_version: u64,
    /// 右侧 overscan 扩展 ticks（补偿 fire-and-forget 模式下 buffer 滞后）
    pub overscan_ticks: f32,
}

/// 发送给 NoteWorker 的作业
pub(crate) struct OnionSkinJob {
    pub snapshot: OnionSkinComputationSnapshot,
    pub onion_note_buffer: Arc<SwappableBuffer<OnionNote>>,
    pub done_tx: Option<mpsc::Sender<()>>,
}

// ─── 滚动速度追踪器 ─────────────────────────────────────────────────────────

/// 滚动速度追踪器——测量 scroll_x 变化率来计算 overscan
///
/// # 设计原则
/// - 取最近 5 帧的 max velocity 而非 avg/EMA
///   ——BPM 可以从 60 突跳到 2000+，EMA 跟不上，max 才能覆盖峰值
/// - 只跟踪向右滚动（FixedIndicatorLeft 模式下 playback 永不回滚）
/// - 100fps 下每 ~10ms 采样，5 帧窗口 ≈ 50ms，覆盖 1 个 worker P50 周期
#[derive(Debug)]
pub(crate) struct ScrollVelocityTracker {
    last_scroll_x: f32,
    last_time: Instant,
    samples: [f32; 5],
    sample_idx: usize,
}

impl ScrollVelocityTracker {
    pub fn new() -> Self {
        Self {
            last_scroll_x: 0.0,
            last_time: Instant::now(),
            samples: [0.0; 5],
            sample_idx: 0,
        }
    }

    /// 更新采样，返回当前峰值速度（ticks/sec）
    pub fn update(&mut self, scroll_x: f32, zoom_x: f32) -> f32 {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time);
        self.last_time = now;

        let dx = scroll_x - self.last_scroll_x;
        self.last_scroll_x = scroll_x;

        // 防除零抖动（2ms 内的采样跳过也不影响，下个采样点补上即可）
        if dt < Duration::from_millis(2) || zoom_x <= 0.0 {
            return self.peak_velocity();
        }

        let dt_sec = dt.as_secs_f32();
        let dx_ticks = dx / zoom_x;
        // 只跟踪向右滚动（playback 方向），向左拖拽不触发 overscan
        let velocity = if dx_ticks > 0.0 {
            dx_ticks / dt_sec
        } else {
            0.0
        };

        self.samples[self.sample_idx % 5] = velocity;
        self.sample_idx += 1;
        self.peak_velocity()
    }

    fn peak_velocity(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// 计算需要的右侧 overscan ticks
    ///
    /// * `predict_ms` — 预测窗口（毫秒），建议设为 worker P95 + 余量（如 60ms）
    pub fn overscan_ticks(&self, predict_ms: f32) -> f32 {
        self.peak_velocity() * predict_ms / 1000.0
    }
}

impl Default for ScrollVelocityTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── NoteWorker ─────────────────────────────────────────────────────────────

/// 洋葱皮计算 coordinator 线程
pub(crate) struct NoteWorker {
    sender: mpsc::Sender<OnionSkinJob>,
    _thread: thread::JoinHandle<()>,
}

/// 每个 Worker 持有的渲染游标状态
///
/// 与 `OnionSkinBucket` 分离，使桶本身只读、可跨线程共享。
struct CursorState {
    /// 每个 key 的扫描游标
    cursors: Box<[usize; 256]>,
    /// 上一帧的 tick_start，用于检测时间回退
    last_tick_start: f32,
    /// 上一次使用的 bucket 版本号
    last_bucket_version: u64,
}

impl CursorState {
    fn new() -> Self {
        Self {
            cursors: Box::new([0; 256]),
            last_tick_start: 0.0,
            last_bucket_version: u64::MAX,
        }
    }

    /// 数据变化或时间回退时重置游标
    fn reset(&mut self, bucket_version: u64, tick_start: f32) {
        self.cursors.fill(0);
        self.last_bucket_version = bucket_version;
        self.last_tick_start = tick_start;
    }
}

impl NoteWorker {
    pub fn spawn() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<OnionSkinJob>();
        let _thread = thread::Builder::new()
            .name("onion-coordinator".into())
            .spawn(move || Self::run_coordinator(rx))
            .map_err(|e| tracing::error!("Failed to spawn NoteWorker: {}", e))
            .ok()?;
        Some(Self {
            sender: tx,
            _thread,
        })
    }

    fn run_coordinator(rx: mpsc::Receiver<OnionSkinJob>) {
        let mut cursor_state = CursorState::new();

        loop {
            let mut job = match rx.recv() {
                Ok(j) => j,
                Err(_) => {
                    tracing::info!("NoteWorker: channel closed, exiting");
                    break;
                }
            };
            // Drain 积压，只保留最新
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            let snap = &job.snapshot;

            // 洋葱皮关闭 → 清空 buffer
            if !snap.onion_skin_enabled {
                unsafe { job.onion_note_buffer.write_buffer() }.clear();
                job.onion_note_buffer.swap();
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            }

            let Some(bucket) = &snap.onion_bucket else {
                tracing::debug!("NoteWorker: no onion bucket available, clearing buffer");
                unsafe { job.onion_note_buffer.write_buffer() }.clear();
                job.onion_note_buffer.swap();
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            };

            // 数据变化时重置游标；时间回退由 collect_visible_with_cursor 内部处理
            if snap.bucket_version != cursor_state.last_bucket_version {
                cursor_state.reset(snap.bucket_version, snap.visible_tick_start);
            }

            // ── Phase 1: 右侧 overscan（补偿 fire-and-forget buffer 滞后） ──
            // 当用户播放时，scroll_x 每帧都在右移，但 NoteWorker 计算出的 buffer
            // 总是对应 ~40ms 前的视口位置。右侧 overscan 提前算好即将出现的音符，
            // 让 buffer 即使落后 1 个 worker 周期也能覆盖实际视口。
            const MAX_VISIBLE_NOTES: usize = 3_000_000;
            let viewport_width = snap.visible_tick_end - snap.visible_tick_start;
            let right_pad = snap.overscan_ticks.min(viewport_width * 1.5);
            let extended_end = (snap.visible_tick_end + right_pad).max(0.0);

            let buf = unsafe { job.onion_note_buffer.write_buffer() };
            buf.clear();

            let current_track_u16 = snap.current_track as u16;
            let track_states = &snap.track_onion_states;
            let track_filter = |track_idx: u16| {
                track_idx != current_track_u16
                    && track_states
                        .get(&(track_idx as usize))
                        .copied()
                        .unwrap_or(true)
            };

            bucket.collect_visible_with_cursor(
                OnionCollectParams::new(
                    snap.visible_tick_start,
                    extended_end,
                    snap.visible_key_min,
                    snap.visible_key_max,
                    cursor_state.last_tick_start,
                ),
                &mut cursor_state.cursors,
                track_filter,
                buf,
            );

            // 简单上限保护，避免极端视口下内存爆炸
            if buf.len() > MAX_VISIBLE_NOTES {
                buf.truncate(MAX_VISIBLE_NOTES);
            }

            // ── Phase 2: 自适应排序（小 N 用 seq sort 避免 rayon 开销） ──
            if buf.len() < 100_000 {
                buf.sort_unstable_by_key(|n| n.start_tick);
            } else {
                use rayon::prelude::*;
                buf.par_sort_unstable_by_key(|n| n.start_tick);
            }

            cursor_state.last_tick_start = snap.visible_tick_start;

            job.onion_note_buffer.swap();

            if let Some(tx) = job.done_tx {
                let _ = tx.send(());
            }
        }
    }

    pub fn send(&self, job: OnionSkinJob) {
        if let Err(e) = self.sender.send(job) {
            tracing::warn!("NoteWorker: send failed: {}", e);
        }
    }

    #[cfg(test)]
    pub fn shutdown(self) {
        drop(self.sender);
    }
}

// ─── 主音轨主音符实例构建（主线程同步执行） ─────────────────────────────────

pub(super) fn build_main_note_instances(
    buffer: &SwappableBuffer<lumino_gfx::NoteInstance>,
    notes: &im::Vector<crate::editor::note::Note>,
    edit_state: &crate::editor::editor_state::EditState,
    default_note_length: f32,
    snap_precision: f32,
) {
    use rayon::prelude::*;
    let instances = unsafe { buffer.write_buffer() };
    instances.clear();
    instances.reserve(notes.len() + 1);

    let main: Vec<lumino_gfx::NoteInstance> = notes
        .par_iter()
        .map(|note| {
            lumino_gfx::NoteInstance::new(
                note.tick,
                note.key as f32,
                note.length,
                [0.2, 0.5, 1.0, 0.9],
            )
        })
        .collect();
    instances.extend(main);

    const DRAWING_NOTE_COLOR: [f32; 4] = [0.4, 0.8, 1.0, 1.0];
    if let crate::editor::editor_state::EditState::Drawing {
        start_tick,
        key,
        current_tick,
    } = edit_state
    {
        let (tick, length) = if *current_tick > *start_tick {
            (*start_tick, *current_tick - *start_tick)
        } else if *current_tick < *start_tick {
            (*current_tick, *start_tick - *current_tick)
        } else {
            (*start_tick, default_note_length)
        };
        instances.push(lumino_gfx::NoteInstance::new(
            tick,
            *key as f32,
            length.max(snap_precision),
            DRAWING_NOTE_COLOR,
        ));
    }
    buffer.swap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// 基准测试（外部文件，仅 test 编译）
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[path = "note_worker_bench.rs"]
mod bench;
