//! 洋葱皮计算专用线程
//!
//! 架构说明：
//! - `NoteWorker` 是一个常驻线程，只负责洋葱皮实例计算
//! - 主音轨主音符 → 由主线程同步写入 `note_instances_buffer` 并 swap（~1ms，保证立即可见）
//! - 洋葱皮数据 → 由 Worker 异步写入独立的 `onion_skin_instances_buffer`（50-200ms）
//! - WGPU 渲染线程分别检测两个 buffer 的版本号，合并后上传
//!
//! 数据流：
//!   Main Thread                  NoteWorker              WGPU Thread
//!     │                             │                       │
//!     ├─ write main notes ──────────│                       │
//!     ├─ swap() ────────────────────│──── 立即可见 ────────►│
//!     │                             │                       ├─ version check
//!     ├─ dispatch snapshot ────────►│                       │
//!     │                             ├─ compute_onion        │
//!     │                             ├─ write onion buffer   │
//!     │                             └─ swap() ─────────────►│
//!     │                                                     ├─ merge + upload
//!     │                                                     └─ render

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use lumino_gfx::{NoteInstance, SwappableBuffer};

use crate::editor::compute_onion_skin_instances_standalone;
use crate::editor::onion_skin::OnionSkinConfig;
use crate::editor::onion_skin_cache_version;
use lumino_core::midi::MidiDocument;

// ─── 数据快照 ───────────────────────────────────────────────────────────────

/// 洋葱皮计算所需的全部数据快照（Send 安全）
///
/// 主线程在每帧收集快照后发送给 worker。
pub(crate) struct OnionSkinComputationSnapshot {
    // 视口参数（用于洋葱皮过滤）
    pub visible_tick_start: f32,
    pub visible_tick_end: f32,
    pub visible_key_min: u16,
    pub visible_key_max: u16,
    // 洋葱皮数据
    pub onion_skin_enabled: bool,
    pub track_onion_states: std::collections::HashMap<usize, bool>,
    pub current_track: usize,
    pub onion_skin_config: OnionSkinConfig,
    pub document: Option<Arc<MidiDocument>>,
}

/// 发送给 NoteWorker 的作业
pub(crate) struct OnionSkinJob {
    pub snapshot: OnionSkinComputationSnapshot,
    /// 目标洋葱皮双缓冲（独立 buffer，不碰主音符）
    pub onion_skin_buffer: Arc<SwappableBuffer<NoteInstance>>,
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

    /// Worker 主循环：接收作业 → 计算洋葱皮 → 写入独立 buffer → swap
    fn run_loop(rx: mpsc::Receiver<OnionSkinJob>) {
        // 缓存版本号追踪：跳过无变更的 swap，避免 WGPU 不必要重传
        let mut last_cache_version: u64 = 0;

        loop {
            // 阻塞等待第一个作业
            let mut job = match rx.recv() {
                Ok(j) => j,
                Err(_) => {
                    tracing::info!("NoteWorker: channel closed, exiting");
                    break;
                }
            };

            // Drain 队列中积压的旧作业，只保留最新的
            // 快速滚动时积压的旧帧洋葱皮数据全部丢弃
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            // 计算洋葱皮
            let snapshot = &job.snapshot;
            let onion_instances = compute_onion_skin_instances_standalone(
                snapshot.onion_skin_enabled,
                snapshot.document.as_ref(),
                &snapshot.onion_skin_config,
                &snapshot.track_onion_states,
                snapshot.current_track,
                snapshot.visible_tick_start,
                snapshot.visible_tick_end,
                snapshot.visible_key_min,
                snapshot.visible_key_max,
            );

            // 检查缓存版本号：无变化时跳过 swap
            let current_cache_version = onion_skin_cache_version();
            if current_cache_version == last_cache_version {
                // 缓存未变化 → 跳过 write + swap，节省 GPU 带宽
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            }
            last_cache_version = current_cache_version;

            // 写入独立的洋葱皮 buffer
            {
                let buffer = unsafe { job.onion_skin_buffer.write_buffer() };
                buffer.clear();
                buffer.reserve(onion_instances.len());
                buffer.extend(onion_instances);
            }
            job.onion_skin_buffer.swap();

            // 通知调用者
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

// ─── 主音轨主音符实例构建（主线程同步执行） ─────────────────────────────────

/// 主线程同步构建主音轨音符实例（~1ms，不阻塞渲染）
///
/// 直接在双缓冲后缓冲区写入并 swap，保证 WGPU 线程立即可见。
/// 不依赖洋葱皮，不给 worker 增加负载。
pub(super) fn build_main_note_instances(
    buffer: &SwappableBuffer<NoteInstance>,
    notes: &im::Vector<crate::editor::note::Note>,
    edit_state: &crate::editor::editor_state::interaction::EditState,
    default_note_length: f32,
    snap_precision: f32,
) {
    use rayon::prelude::*;

    let instances = unsafe { buffer.write_buffer() };
    instances.clear();
    instances.reserve(notes.len() + 1);

    // 并行构建主音轨音符
    let main: Vec<NoteInstance> = notes
        .par_iter()
        .map(|note| {
            NoteInstance::new(
                note.tick,
                note.key as f32,
                note.length,
                [0.2, 0.5, 1.0, 0.9],
            )
        })
        .collect();
    instances.extend(main);

    // 绘制中音符（单线程，最多1个）
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
        instances.push(NoteInstance::new(
            tick,
            *key as f32,
            length,
            DRAWING_NOTE_COLOR,
        ));
    }

    buffer.swap();
}
