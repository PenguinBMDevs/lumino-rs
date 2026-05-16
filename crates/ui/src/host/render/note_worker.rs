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

    /// Worker 主循环：接收作业 → 只处理最新一条 → 执行计算
    fn run_loop(rx: mpsc::Receiver<NoteComputationJob>) {
        loop {
            // 阻塞等待第一条作业
            let mut job = match rx.recv() {
                Ok(j) => j,
                Err(_) => {
                    tracing::info!("NoteWorker: channel closed, exiting");
                    break;
                }
            };

            // Drain 队列中积压的旧作业，只保留最新的
            // 渲染场景中，旧的帧数据不应该被处理
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }

            // 执行计算
            Self::process_job(&job);

            // 如果调用者在等待同步信号，通知它
            if let Some(tx) = job.done_tx {
                let _ = tx.send(());
            }
        }
    }

    /// 处理单个作业：洋葱皮 → 主音符 → 绘制中音符 → 双缓冲 swap
    fn process_job(job: &NoteComputationJob) {
        let snapshot = &job.snapshot;

        // Step 1: 计算洋葱皮实例（独立版，不依赖 &mut Editor）
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

        // Step 2: 获取双缓冲后缓冲区写入引用
        let instances = unsafe { job.buffer.write_buffer() };
        instances.clear();

        // 预分配容量：主音符 + 洋葱皮 + 绘制中音符（最多1个）
        let total_reserve = snapshot.notes.len() + onion_instances.len() + 1;
        instances.reserve(total_reserve);

        // Step 3: 添加主要音符（全量送入 GPU，由 shader 裁剪）
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        add_notes_to_instances(instances, &snapshot.notes, DEFAULT_NOTE_COLOR);

        // Step 4: 添加洋葱皮音符
        instances.extend(onion_instances);

        // Step 5: 添加正在绘制的音符
        add_drawing_note_to_instances(
            instances,
            &snapshot.edit_state,
            snapshot.default_note_length,
            snapshot.snap_precision,
        );

        // Step 6: 交换双缓冲区，使新数据对渲染线程可见
        job.buffer.swap();
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

/// 将音符添加到实例列表
///
/// 优化说明（针对火焰图 78ms 瓶颈）：
/// 1. 始终使用 `.iter()` 顺序遍历 im::Vector（均摊 O(1) 每元素），
///    避免 rayon 并行时 `notes.get(i)` 带来的 O(log n) 随机访问开销。
///    im::Vector 的 RRB 树结构使随机访问需要指针追踪 3-4 层，
///    在大数据量下比顺序迭代慢 5-10 倍。
/// 2. 预分配容量避免重复扩容。
/// 3. 消除 per-thread Vec 分配 + reduce 合并的额外开销。
pub(super) fn add_notes_to_instances(
    instances: &mut Vec<lumino_gfx::NoteInstance>,
    notes: &im::Vector<Note>,
    color: [f32; 4],
) {
    for note in notes.iter() {
        instances.push(lumino_gfx::NoteInstance::new(
            note.tick,
            note.key as f32,
            note.length,
            color,
        ));
    }
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
