//! 洋葱皮计算专用线程 — GPU OnionNote 生成模式
//!
//! 架构说明：
//! - `NoteWorker` 是一个常驻线程，负责收集所有可见洋葱皮音轨的音符数据
//! - 主音轨主音符 → 由主线程同步写入 `note_instances_buffer` 并 swap（~1ms，保证立即可见）
//! - 洋葱皮数据 → 由 Worker 异步收集 OnionNote 并写入 `onion_note_buffer`（50-200ms）
//! - WGPU 渲染线程检测 onion_note_buffer 版本号，上传到 OnionRenderer 的 storage buffer
//!
//! 数据流：
//!   Main Thread                  NoteWorker              WGPU Thread
//!     │                             │                       │
//!     ├─ write main notes ──────────│                       │
//!     ├─ swap() ────────────────────│──── 立即可见 ────────►│
//!     │                             │                       ├─ version check
//!     ├─ dispatch snapshot ────────►│                       │
//!     │                             ├─ collect OnionNotes   │
//!     │                             ├─ write + swap         │
//!     │                             │── swap() ────────────►│
//!     │                             │                       ├─ upload to OnionRenderer
//!     │                             │                       ├─ compute cull + draw

use std::sync::{Arc, mpsc};
use std::thread;

use lumino_core::midi::MidiDocument;
use lumino_gfx::{OnionNote, SwappableBuffer};

// ─── 数据快照 ───────────────────────────────────────────────────────────────

/// 洋葱皮计算所需的全部数据快照（Send 安全）
///
/// 主线程在每帧收集快照后发送给 worker。
#[allow(dead_code)]
pub(crate) struct OnionSkinComputationSnapshot {
    // 视口参数（用于洋葱皮过滤与 NDC 坐标计算）
    pub visible_tick_start: f32,
    pub visible_tick_end: f32,
    pub visible_key_min: u16,
    pub visible_key_max: u16,
    // ─── 以下字段用于 NDC 坐标计算 ───
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
    // 洋葱皮数据
    pub onion_skin_enabled: bool,
    pub track_onion_states: std::collections::HashMap<usize, bool>,
    pub current_track: usize,
    pub document: Option<Arc<MidiDocument>>,
}

/// 发送给 NoteWorker 的作业
pub(crate) struct OnionSkinJob {
    pub snapshot: OnionSkinComputationSnapshot,
    /// 洋葱皮音符池双缓冲（SoA 布局，OnionNote 类型）
    pub onion_note_buffer: Arc<SwappableBuffer<OnionNote>>,
    /// 同步信号：单线程模式用，等待完成后通知
    pub done_tx: Option<mpsc::Sender<()>>,
}

// ─── Worker 线程 ────────────────────────────────────────────────────────────

/// 洋葱皮计算专用线程
pub(crate) struct NoteWorker {
    sender: mpsc::Sender<OnionSkinJob>,
    _thread: thread::JoinHandle<()>,
}

impl NoteWorker {
    /// 创建并启动 worker 线程
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<OnionSkinJob>();
        let _thread = thread::Builder::new()
            .name("onion-worker".into())
            .spawn(move || Self::run_loop(rx))
            .expect("Failed to spawn NoteWorker thread");

        Self {
            sender: tx,
            _thread,
        }
    }

    /// Worker 主循环：接收作业 → 收集 OnionNote → 写入双缓冲 → swap
    fn run_loop(rx: mpsc::Receiver<OnionSkinJob>) {
        loop {
            let mut job = match rx.recv() {
                Ok(j) => j,
                Err(_) => {
                    tracing::info!("NoteWorker: channel closed, exiting");
                    break;
                }
            };

            // Drain 队列中积压的旧作业，只保留最新的
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            let snapshot = &job.snapshot;

            if !snapshot.onion_skin_enabled {
                let buffer = unsafe { job.onion_note_buffer.write_buffer() };
                buffer.clear();
                let _ = buffer;
                job.onion_note_buffer.swap();
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            }

            // 收集所有可见洋葱皮音轨的音符，转为 OnionNote
            let notes = collect_onion_notes(
                snapshot.document.as_deref(),
                snapshot.current_track,
                &snapshot.track_onion_states,
            );

            {
                let buffer = unsafe { job.onion_note_buffer.write_buffer() };
                buffer.clear();
                buffer.extend(notes);
            }
            job.onion_note_buffer.swap();

            if let Some(tx) = job.done_tx {
                let _ = tx.send(());
            }
        }
    }

    /// 发送洋葱皮计算作业（非阻塞）
    pub fn send(&self, job: OnionSkinJob) {
        if let Err(e) = self.sender.send(job) {
            tracing::warn!("NoteWorker: failed to send job: {}", e);
        }
    }

    /// 关闭 worker 线程
    #[allow(dead_code)]
    pub fn shutdown(self) {
        drop(self.sender);
    }
}

// ─── OnionNote 收集 ─────────────────────────────────────────────────────────

/// 从 MIDI 文档收集所有非当前轨道的洋葱皮音符
///
/// 使用 Rayon 并行遍历音轨，收集后按 start_tick 排序
/// 排序后的音符池使 GPU cull shader 可使用二分查找加速
pub(super) fn collect_onion_notes(
    document: Option<&MidiDocument>,
    current_track: usize,
    track_onion_enabled: &std::collections::HashMap<usize, bool>,
) -> Vec<OnionNote> {
    let Some(doc) = document else {
        return Vec::new();
    };

    use rayon::prelude::*;
    let num_tracks = doc.track_count();

    // 并行遍历音轨，每个轨道生成独立 Vec，最后合并排序
    let track_results: Vec<Vec<OnionNote>> = (0..num_tracks)
        .into_par_iter()
        .filter_map(|track_idx| {
            if track_idx == current_track {
                return None;
            }
            let is_onion = track_onion_enabled.get(&track_idx).copied().unwrap_or(true);
            if !is_onion {
                return None;
            }

            let track_idx_u16 = track_idx as u16;
            let track_notes = doc.get_track_notes(track_idx as u16);
            let mut track_notes_vec = Vec::with_capacity(track_notes.len());
            for (start, pitch, duration, _, _) in track_notes {
                let start_u32 = start as u32;
                let end = start_u32 + duration as u32;
                track_notes_vec.push(OnionNote::new(start_u32, end, pitch, track_idx_u16));
            }
            Some(track_notes_vec)
        })
        .collect();

    let total: usize = track_results.iter().map(|v| v.len()).sum();
    let mut notes = Vec::with_capacity(total);
    for v in track_results {
        notes.extend(v);
    }

    // 按 start_tick 排序，GPU cull shader 可用二分查找定位可见范围
    notes.sort_unstable_by_key(|n| n.start_tick);

    notes
}

// ─── 主音轨主音符实例构建（主线程同步执行） ─────────────────────────────────

/// 主线程同步构建主音轨音符实例（~1ms，不阻塞渲染）
pub(super) fn build_main_note_instances(
    buffer: &SwappableBuffer<lumino_gfx::NoteInstance>,
    notes: &im::Vector<crate::editor::note::Note>,
    edit_state: &crate::editor::editor_state::interaction::EditState,
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
    if let crate::editor::editor_state::interaction::EditState::Drawing {
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
        let length = length.max(snap_precision);
        instances.push(lumino_gfx::NoteInstance::new(
            tick,
            *key as f32,
            length,
            DRAWING_NOTE_COLOR,
        ));
    }

    buffer.swap();
}
