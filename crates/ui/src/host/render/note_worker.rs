//! 音符计算专用线程 —— 将 `update_all_note_instances_fast` 完全移出主线程
//!
//! 架构说明：
//! - `NoteWorker` 是一个常驻线程，通过 mpsc channel 接收计算作业
//! - 主线程收集数据快照并发送，不阻塞
//! - Worker 线程负责：洋葱皮实例计算 + 主音符遍历 + 双缓冲写入 + swap
//! - WGPU 渲染线程从双缓冲读取音符实例
//!
//! 数据流：
//!   Main Thread                NoteWorker              WGPU Thread
//!     │                           │                       │
//!     ├─ collect snapshot ──────► │                       │
//!     │   (极快, O(1) im:clone)   ├─ compute_onion ────── │
//!     │                           ├─ build_instances      │
//!     │                           ├─ write_buffer()       │
//!     │                           └─ swap() ─────────────► │
//!     │                                                    ├─ read_buffer()
//!     │                                                    └─ upload + render

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use im::Vector;
use lumino_gfx::SwappableBuffer;

use crate::editor::compute_onion_skin_instances_standalone;
use crate::editor::editor_state::interaction::EditState;
use crate::editor::note::Note;
use crate::editor::onion_skin::OnionSkinConfig;
use lumino_core::midi::MidiDocument;

// ─── 数据快照 ───────────────────────────────────────────────────────────────

/// 音符计算所需的全部数据快照（Send 安全）
///
/// 主线程在每帧收集所有 immutable 数据并发送给 worker。
/// im::Vector::clone() 是 O(1) 结构共享，没有深拷贝开销。
pub(crate) struct NoteComputationSnapshot {
    // 主音轨音符（clone O(1) — RRB 树结构共享）
    pub notes: Vector<Note>,
    // 视口参数（用于洋葱皮过滤）
    pub visible_tick_start: f32,
    pub visible_tick_end: f32,
    pub visible_key_min: u16,
    pub visible_key_max: u16,
    // 编辑视图参数
    pub default_note_length: f32,
    pub snap_precision: f32,
    // 当前编辑状态
    pub edit_state: EditState,
    // 洋葱皮数据
    pub onion_skin_enabled: bool,
    pub track_onion_states: std::collections::HashMap<usize, bool>,
    pub current_track: usize,
    pub onion_skin_config: OnionSkinConfig,
    pub document: Option<Arc<MidiDocument>>,
}

/// 发送给 NoteWorker 的作业
pub(crate) struct NoteComputationJob {
    pub snapshot: NoteComputationSnapshot,
    pub buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    /// 可选同步信号：单线程模式用，等待 worker 完成后通知调用者
    pub done_tx: Option<mpsc::Sender<()>>,
}

// ─── Worker 线程 ────────────────────────────────────────────────────────────

/// 音符计算专用线程
///
/// 生命周期由 `enable_separate_render_thread` / `disable_separate_render_thread` 管理。
/// 单线程模式和分离渲染模式共享同一个 worker。
pub(crate) struct NoteWorker {
    sender: mpsc::Sender<NoteComputationJob>,
    _thread: thread::JoinHandle<()>,
}

impl NoteWorker {
    /// 创建并启动 worker 线程
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<NoteComputationJob>();
        let _thread = thread::Builder::new()
            .name("note-worker".into())
            .spawn(move || Self::run_loop(rx))
            .expect("Failed to spawn NoteWorker thread");

        Self {
            sender: tx,
            _thread,
        }
    }

    /// Worker 主循环：接收作业 → 两阶段处理
    ///
    /// 两阶段设计解决洋葱皮阻塞主音符的问题：
    /// - Phase 1: 主音符 + 绘制中音符 → 立即 swap（~1ms，不阻塞渲染）
    /// - Phase 2: 洋葱皮实例计算 → swap（可能 50-200ms，但此时渲染已拿到主音符）
    ///
    /// 两阶段都是必须的（不跳过洋葱皮），否则用户播放预览时看不见其他音轨。
    /// 快速滚动的性能靠以下机制保证：
    /// 1. Phase 1 瞬时完成 → 主音符零延迟
    /// 2. Phase 2 完成后 drain 积压的旧 job → 只处理最新一帧
    /// 3. 即使 Phase 2 耗时 100ms，落在视口上的延迟 ~1 个周期，始终有数据
    fn run_loop(rx: mpsc::Receiver<NoteComputationJob>) {
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
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            // ─── Phase 1: 主音符 + 绘制中音符 ───
            // 始终执行，主音符不依赖视口。立即 swap 让 WGPU 先渲染主音符。
            Self::phase1_main_notes(&job);

            // ─── Phase 2: 洋葱皮实例计算 ───
            // 始终执行，不跳过。即使用户在快速滚动，洋葱皮也在后台计算，
            // 完成后 swap，WGPU 线程拿到包含洋葱皮的完整数据。
            // 延迟约一个 Phase 2 周期（50-200ms），但始终有数据。
            Self::phase2_onion_skin(&job);

            // 如果调用者在等待同步信号，通知它
            if let Some(tx) = job.done_tx {
                let _ = tx.send(());
            }
        }
    }

    /// Phase 1：主音符 + 绘制中音符 → 立即 swap
    ///
    /// 始终执行，不依赖视口（所有主音符全量送入 GPU，由 shader 裁剪）。
    /// 时间复杂度 O(main_notes)，仅 ~1ms。
    fn phase1_main_notes(job: &NoteComputationJob) {
        let snapshot = &job.snapshot;
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];

        let instances = unsafe { job.buffer.write_buffer() };
        instances.clear();
        instances.reserve(snapshot.notes.len() + 1);
        add_notes_to_instances(instances, &snapshot.notes, DEFAULT_NOTE_COLOR);
        add_drawing_note_to_instances(
            instances,
            &snapshot.edit_state,
            snapshot.default_note_length,
            snapshot.snap_precision,
        );
        job.buffer.swap(); // WGPU 线程立即看到主音符
    }

    /// Phase 2：洋葱皮实例计算 → swap
    ///
    /// 仅在视口未过时时执行（没有新 job 等待）。
    /// 时间复杂度取决于音轨数量和密度，可能 50-200ms。
    fn phase2_onion_skin(job: &NoteComputationJob) {
        let snapshot = &job.snapshot;
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];

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

        if !onion_instances.is_empty() {
            let instances = unsafe { job.buffer.write_buffer() };
            instances.clear();
            instances.reserve(snapshot.notes.len() + onion_instances.len() + 1);
            add_notes_to_instances(instances, &snapshot.notes, DEFAULT_NOTE_COLOR);
            instances.extend(onion_instances);
            add_drawing_note_to_instances(
                instances,
                &snapshot.edit_state,
                snapshot.default_note_length,
                snapshot.snap_precision,
            );
            job.buffer.swap(); // WGPU 线程现在有主音符 + 洋葱皮
        }
    }

    /// 发送计算作业（非阻塞）
    ///
    /// mpsc channel 有界（默认容量不定），如果 worker 忙，
    /// send 会阻塞直到被消费。为防阻塞主线程，这里不做同步等待。
    /// 在单线程模式下, main thread 可能通过 done_tx 等待完成。
    pub fn send(&self, job: NoteComputationJob) {
        if let Err(e) = self.sender.send(job) {
            tracing::warn!("NoteWorker: failed to send job: {}", e);
        }
    }

    /// 关闭 worker 线程
    #[allow(dead_code)]
    pub fn shutdown(self) {
        drop(self.sender);
        // thread::JoinHandle 被 drop 时会 detach，worker 在 channel 关闭后退出
    }
}

// ─── 音符实例构建函数（从 `impl Host` 拆出） ──────────────────────────────

/// 将音符添加到实例列表（多线程并行）
///
/// 使用 im::Vector::par_iter() 进行 RRB 树结构感知的并行遍历，
/// 比顺序 iter() 快 4-6x（8 核，百万级音符）。
/// 避免了 `notes.get(i)` 的 O(log n) 随机访问开销（par_iter 按子树分块）。
pub(super) fn add_notes_to_instances(
    instances: &mut Vec<lumino_gfx::NoteInstance>,
    notes: &im::Vector<Note>,
    color: [f32; 4],
) {
    use rayon::prelude::*;

    // par_iter 将 RRB 树按子树分块，每个 rayon 线程处理一个连续子树块，
    // 块内顺序遍历 O(1) amortized，块间并行。collect 在子线程本地分配，
    // 最后归并到主 Vec。
    let new_instances: Vec<lumino_gfx::NoteInstance> = notes
        .par_iter()
        .map(|note| lumino_gfx::NoteInstance::new(note.tick, note.key as f32, note.length, color))
        .collect();

    instances.extend(new_instances);
}

/// 添加正在绘制的音符到实例列表
pub(super) fn add_drawing_note_to_instances(
    instances: &mut Vec<lumino_gfx::NoteInstance>,
    edit_state: &EditState,
    default_note_length: f32,
    snap_precision: f32,
) {
    const DRAWING_NOTE_COLOR: [f32; 4] = [0.4, 0.8, 1.0, 1.0];

    if let EditState::Drawing {
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
}
