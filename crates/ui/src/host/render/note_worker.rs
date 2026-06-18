//! 洋葱皮计算 coordinator — 无缓存，直接读取 MIDI 文档
//!
//! == 架构 ==
//! `NoteWorker` 是 coordinator 线程，接收 `OnionSkinJob` 后直接处理。
//! 无 per-track 缓存（避免黑乐谱 100M+ 音符的内存爆炸），
//! 直接从 `MidiDocument::track_notes` 读取已排序的 `NoteInfo`，
//! 通过二分查找定位视口可见范围，构造 `OnionNote` 后并行排序。
//!
//! == 数据流 ==
//!   Main Thread              NoteWorker (coordinator)          WGPU Thread
//!     │                            │                               │
//!     ├─ snapshot ────────────────►├─ 遍历音轨                      │
//!     │                            │  ├─ binary search              │
//!     │                            │  ├─ 构建 OnionNote(可见范围)   │
//!     │                            │  └─ par_sort                   │
//!     │                            ├─ swap combined buffer ────────►│
//!     │                            │                               ├─ upload to GPU

use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use lumino_gfx::{OnionNote, SwappableBuffer};
use lumino_midi_loader::MidiDocument;
use lumino_midi_loader::constants::TICK_SEARCH_BUFFER;

// ─── 数据快照 ───────────────────────────────────────────────────────────────

/// 洋葱皮计算所需的全部数据快照（Send 安全）
#[expect(dead_code)]
pub(crate) struct OnionSkinComputationSnapshot {
    pub visible_tick_start: f32,
    pub visible_tick_end: f32,
    pub visible_key_min: u16,
    pub visible_key_max: u16,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub canvas_offset_x: f32,
    pub canvas_offset_y: f32,
    pub viewport_logical_width: f32,
    pub viewport_logical_height: f32,
    pub max_key_index: f32,
    pub onion_skin_enabled: bool,
    pub track_onion_states: HashMap<usize, bool>,
    pub current_track: usize,
    pub document: Option<Arc<MidiDocument>>,
    pub track_notes: Arc<HashMap<usize, im::Vector<crate::editor::note::Note>>>,
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

            // ── Phase 1: 右侧 overscan（补偿 fire-and-forget buffer 滞后） ──
            // 当用户播放时，scroll_x 每帧都在右移，但 NoteWorker 计算出的 buffer
            // 总是对应 ~40ms 前的视口位置。右侧 overscan 提前算好即将出现的音符，
            // 让 buffer 即使落后 1 个 worker 周期也能覆盖实际视口。
            const MAX_VISIBLE_NOTES: usize = 3_000_000;
            let viewport_width = snap.visible_tick_end - snap.visible_tick_start;
            let right_pad = (snap.overscan_ticks).min(viewport_width * 1.5);
            let extended_end = (snap.visible_tick_end + right_pad).max(0.0);

            let buf = unsafe { job.onion_note_buffer.write_buffer() };
            buf.clear();

            // 1a: MIDI 文档音轨
            if let Some(doc) = &snap.document {
                let nt = doc.track_count();
                for ti in 0..nt {
                    if ti == snap.current_track {
                        continue;
                    }
                    if !snap.track_onion_states.get(&ti).copied().unwrap_or(true) {
                        continue;
                    }
                    if snap.track_notes.contains_key(&ti) {
                        continue;
                    }
                    let notes = doc.track_notes(ti);
                    if notes.is_empty() {
                        continue;
                    }
                    collect_visible(notes, ti as u16, snap.visible_tick_start, extended_end, buf);
                    if buf.len() >= MAX_VISIBLE_NOTES {
                        break;
                    }
                }
            }
            // 1b: 用户编辑音轨
            if buf.len() < MAX_VISIBLE_NOTES {
                for (&ti, v) in snap.track_notes.iter() {
                    if ti == snap.current_track {
                        continue;
                    }
                    if !snap.track_onion_states.get(&ti).copied().unwrap_or(true) {
                        continue;
                    }
                    collect_visible_user(v, ti as u16, snap.visible_tick_start, extended_end, buf);
                    if buf.len() >= MAX_VISIBLE_NOTES {
                        break;
                    }
                }
            }

            // ── Phase 2: 自适应排序（小 N 用 seq sort 避免 rayon 开销） ──
            if buf.len() < 100_000 {
                buf.sort_unstable_by_key(|n| n.start_tick);
            } else {
                use rayon::prelude::*;
                buf.par_sort_unstable_by_key(|n| n.start_tick);
            }

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

    #[expect(dead_code)]
    pub fn shutdown(self) {
        drop(self.sender);
    }
}

// ─── 视口范围计算（二分查找） ──────────────────────────────────────────────

/// 将 NoteInfo 中可见范围的音符提取为 OnionNote（直接写入目标 Vec）
fn collect_visible(
    notes: &[lumino_midi_loader::NoteInfo],
    track_idx: u16,
    ts: f32,
    te: f32,
    out: &mut Vec<OnionNote>,
) {
    let ts_u = ts as u32;
    let te_u = te as u32;
    let start = notes.partition_point(|n| n.start_tick < ts_u.saturating_sub(TICK_SEARCH_BUFFER));
    if start >= notes.len() {
        return;
    }
    let end = notes
        .len()
        .min(start + notes[start..].partition_point(|n| n.start_tick <= te_u));
    for n in &notes[start..end] {
        let et = n.end_tick();
        if et > ts_u && n.start_tick < te_u {
            out.push(OnionNote::new(n.start_tick, et, n.key, track_idx));
        }
    }
}

/// 将用户编辑音符中可见范围提取为 OnionNote（直接写入目标 Vec）
fn collect_visible_user(
    notes: &im::Vector<crate::editor::note::Note>,
    track_idx: u16,
    ts: f32,
    te: f32,
    out: &mut Vec<OnionNote>,
) {
    let ts_u = ts as u32;
    let te_u = te as u32;
    for n in notes.iter() {
        let st = n.tick as u32;
        let et = (n.tick + n.length) as u32;
        if et > ts_u && st < te_u {
            out.push(OnionNote::new(st, et, n.key as u8, track_idx));
        }
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
