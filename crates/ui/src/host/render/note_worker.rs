//! 洋葱皮计算专用线程
//!
//! 架构说明：
//! - `NoteWorker` 是一个常驻线程，负责洋葱皮实例计算和背景瓦片生成
//! - 主音轨主音符 → 由主线程同步写入 `note_instances_buffer` 并 swap（~1ms，保证立即可见）
//! - 洋葱皮数据 → 由 Worker 异步写入独立的 `onion_skin_instances_buffer`（50-200ms）
//! - 背景瓦片 → 由 Worker 生成像素并上传到 `onion_bg_tiles_buffer`（100-500ms）
//! - WGPU 渲染线程分别检测各 buffer 的版本号，合并后上传
//!
//! 数据流：
//!   Main Thread                  NoteWorker              WGPU Thread
//!     │                             │                       │
//!     ├─ write main notes ──────────│                       │
//!     ├─ swap() ────────────────────│──── 立即可见 ────────►│
//!     │                             │                       ├─ version check
//!     ├─ dispatch snapshot ────────►│                       │
//!     │                             ├─ compute_onion (空)   │
//!     │                             ├─ write empty + swap   │
//!     │                             ├─ tile loop            │
//!     │                             │  ├─ cache hit → ref   │
//!     │                             │  ├─ cache miss → gen  │
//!     │                             │  │  + upload + cache  │
//!     │                             ├─ write tile refs      │
//!     │                             └─ swap() ─────────────►│
//!     │                                                     ├─ merge + upload
//!     │                                                     └─ render

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;

use lumino_gfx::{NoteInstance, OnionBgTileRef, SwappableBuffer};

use crate::editor::onion_bg_lod0::{generate_lod0_pixels, upload_lod0_to_gpu};
use crate::editor::onion_bg_pool::{OnionBgTileMeta, OnionBgTilePool};
use lumino_core::midi::MidiDocument;

/// 瓦片缓存条目：关联 pool 索引和元数据
struct TileCacheEntry {
    pool_index: u16,
    meta: OnionBgTileMeta,
}

// ─── 常量 ────────────────────────────────────────────────────────────────────

/// 瓦片宽度（像素）
const TILE_PIXEL_WIDTH: u32 = 1024;
/// 瓦片高度（像素）
const TILE_PIXEL_HEIGHT: u32 = 512;
/// 每像素对应的 tick 数（LOD0 分辨率，2 ticks = 1 px）
const TICKS_PER_PIXEL: f32 = 2.0;
/// 每像素对应的 key 数（32 px = 1 key）
const PIXELS_PER_KEY: f32 = 32.0;
/// 每个瓦片覆盖的 tick 宽度
const TILE_TICK_WIDTH: f32 = TILE_PIXEL_WIDTH as f32 * TICKS_PER_PIXEL;
/// 每个瓦片覆盖的 key 高度
const TILE_KEY_HEIGHT: u16 = (TILE_PIXEL_HEIGHT as f32 / PIXELS_PER_KEY) as u16;

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
    // ─── 以下字段用于 NDC 坐标计算（与 note.wgsl 保持一致） ───
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
    /// 目标洋葱皮双缓冲（独立 buffer，不碰主音符）
    pub onion_skin_buffer: Arc<SwappableBuffer<NoteInstance>>,
    /// 洋葱皮背景瓦片引用双缓冲
    pub onion_bg_tiles_buffer: Arc<SwappableBuffer<OnionBgTileRef>>,
    /// 共享瓦片纹理池（主线程创建，worker 与 WGPU 线程共用）
    pub tile_pool: Option<Arc<Mutex<OnionBgTilePool>>>,
    /// 同步信号：单线程模式用，等待完成后通知
    pub done_tx: Option<mpsc::Sender<()>>,
}

// ─── 瓦片工具函数 ────────────────────────────────────────────────────────────

/// 计算瓦片 ID（基于 tick/key 分区坐标的哈希）
fn tile_id(tick_div: u32, key_div: u16) -> u64 {
    ((tick_div as u64) << 32) | (key_div as u64)
}

/// 收集当前视口内所有可见瓦片的参数
fn collect_visible_tiles(
    tick_start: f32,
    tick_end: f32,
    key_min: u16,
    key_max: u16,
) -> Vec<(u64, f32, f32, u16, u16, u32, u16)> {
    // 按 TILE_TICK_WIDTH 和 TILE_KEY_HEIGHT 分块
    let tick_div_start = (tick_start / TILE_TICK_WIDTH).floor() as u32;
    let tick_div_end = ((tick_end + TILE_TICK_WIDTH - 1.0) / TILE_TICK_WIDTH).floor() as u32;
    let key_div_start = key_min / TILE_KEY_HEIGHT;
    let key_div_end = (key_max + TILE_KEY_HEIGHT - 1) / TILE_KEY_HEIGHT;

    let mut tiles = Vec::new();

    for td in tick_div_start..tick_div_end {
        let tile_tick_start = td as f32 * TILE_TICK_WIDTH;
        let tile_tick_end = ((td as f32 + 1.0) * TILE_TICK_WIDTH).min(tick_end);

        for kd in key_div_start..key_div_end {
            let tile_key_min = kd * TILE_KEY_HEIGHT;
            let tile_key_max = ((kd + 1) * TILE_KEY_HEIGHT - 1).min(key_max);

            let id = tile_id(td, kd);
            tiles.push((id, tile_tick_start, tile_tick_end, tile_key_min, tile_key_max, td, kd));
        }
    }

    tiles
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

    /// Worker 主循环：接收作业 → 计算瓦片 → 写入双缓冲 → swap
    fn run_loop(rx: mpsc::Receiver<OnionSkinJob>) {
        // 瓦片缓存：tile_id → (pool_index, metadata)
        let mut tile_cache: HashMap<u64, TileCacheEntry> = HashMap::new();

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

            let snapshot = &job.snapshot;
            tracing::debug!(
                "[CK1] job received: viewport=({:.0},{:.0}) keys=[{},{}] pool={}",
                snapshot.visible_tick_start, snapshot.visible_tick_end,
                snapshot.visible_key_min, snapshot.visible_key_max,
                job.tile_pool.is_some(),
            );

            // ═══ 兼容冻结接口：写入空数组到旧洋葱皮 buffer ═══
            {
                let buffer = unsafe { job.onion_skin_buffer.write_buffer() };
                buffer.clear();
            }
            job.onion_skin_buffer.swap();

            // ═══ 瓦片生成逻辑 ═══

            let Some(ref pool) = job.tile_pool else {
                tracing::warn!("[CK1] tile_pool is None, skipping tile generation");
                // 还没拿到瓦片池，跳过瓦片生成
                // 同时把 onion_bg_tiles_buffer 清空（兼容旧 wait）
                {
                    let buffer = unsafe { job.onion_bg_tiles_buffer.write_buffer() };
                    buffer.clear();
                }
                job.onion_bg_tiles_buffer.swap();
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            };

            let mut pool_guard = match pool.lock() {
                Ok(g) => g,
                Err(_) => {
                    if let Some(tx) = job.done_tx {
                        let _ = tx.send(());
                    }
                    continue;
                }
            };

            tracing::debug!("[CK1] collecting visible tiles for viewport ({:.0},{:.0}) keys=[{},{}]",
                snapshot.visible_tick_start, snapshot.visible_tick_end,
                snapshot.visible_key_min, snapshot.visible_key_max);

            // 收集当前视口的可见瓦片
            let visible_tiles = collect_visible_tiles(
                snapshot.visible_tick_start,
                snapshot.visible_tick_end,
                snapshot.visible_key_min,
                snapshot.visible_key_max,
            );

            tracing::debug!("[CK1] visible_tiles count={}", visible_tiles.len());
            if visible_tiles.is_empty() {
                // 清空 buffer 并通知
                {
                    let buffer = unsafe { job.onion_bg_tiles_buffer.write_buffer() };
                    buffer.clear();
                }
                job.onion_bg_tiles_buffer.swap();
                if let Some(tx) = job.done_tx {
                    let _ = tx.send(());
                }
                continue;
            }

            // 构建当前帧的瓦片引用列表
            let mut tile_refs: Vec<OnionBgTileRef> = Vec::with_capacity(visible_tiles.len());
            let mut cache_hits = 0u32;
            let mut cache_misses = 0u32;

            // ─── NDC 计算辅助闭包（匹配 note.wgsl 的坐标变换公式） ───
            // note.wgsl:
            //   screen_x = tick * zoom.x - scroll.x + keyboard_width + canvas_offset.x
            //   screen_y = (max_key_index - key) * zoom.y - scroll.y + ruler_height + canvas_offset.y
            //   ndc_x = screen_x / viewport_size.x * 2 - 1
            //   ndc_y = 1 - screen_y / viewport_size.y * 2
            let tile_to_ndc = |tick_start: f32, tick_end: f32, key_min: u16, key_max: u16| {
                let screen_x = (tick_start * snapshot.zoom_x - snapshot.scroll_x)
                    + snapshot.keyboard_width + snapshot.canvas_offset_x;
                let ndc_x = (screen_x / snapshot.viewport_logical_width) * 2.0 - 1.0;
                let ndc_w = ((tick_end - tick_start) * snapshot.zoom_x / snapshot.viewport_logical_width) * 2.0;

                let screen_y_bottom = (snapshot.max_key_index - key_min as f32) * snapshot.zoom_y
                    - snapshot.scroll_y + snapshot.ruler_height + snapshot.canvas_offset_y;
                let ndc_y = 1.0 - (screen_y_bottom / snapshot.viewport_logical_height) * 2.0;
                let ndc_h = ((key_max - key_min + 1) as f32 * snapshot.zoom_y / snapshot.viewport_logical_height) * 2.0;

                ([ndc_x, ndc_y], [ndc_w, ndc_h])
            };

            for (id, tile_tick_start, tile_tick_end, tile_key_min, tile_key_max, _td, _kd) in
                &visible_tiles
            {
                let id = *id;
                let tile_tick_start = *tile_tick_start;
                let tile_tick_end = *tile_tick_end;
                let tile_key_min = *tile_key_min;
                let tile_key_max = *tile_key_max;

                // 查缓存
                if let Some(entry) = tile_cache.get(&id) {
                    cache_hits += 1;
                    let (pos, size) = tile_to_ndc(
                        entry.meta.tick_range.0,
                        entry.meta.tick_range.1,
                        entry.meta.key_range.0,
                        entry.meta.key_range.1,
                    );
                    tile_refs.push(OnionBgTileRef {
                        position: pos,
                        size,
                        track_index: entry.pool_index as u32,
                        _padding: 0,
                    });
                    continue;
                }

                // 缓存未命中：分配纹理池，生成像素，上传
                cache_misses += 1;
                let Some(pool_idx) = pool_guard.alloc() else {
                    tracing::warn!("NoteWorker: tile pool exhausted, skipping tile");
                    continue;
                };

                // 生成像素
                let pixel_data = generate_lod0_pixels(
                    snapshot.document.as_ref(),
                    snapshot.current_track,
                    tile_tick_start,
                    tile_tick_end,
                    tile_key_min,
                    tile_key_max,
                );
                tracing::info!("[UPLOAD] generated pixels: {}x{} count={}", pixel_data.width, pixel_data.height, pixel_data.note_count);

                // 空像素 → 不缓存、不上传、释放 pool 槽位
                if pixel_data.width == 0 || pixel_data.height == 0 {
                    pool_guard.free(pool_idx);
                    continue;
                }

                // 上传到 GPU（pool 内部持有 queue）
                upload_lod0_to_gpu(&pixel_data, pool_idx, &mut *pool_guard);

                // 写入缓存
                let meta = OnionBgTileMeta {
                    tile_id: id,
                    tick_range: (tile_tick_start, tile_tick_end),
                    key_range: (tile_key_min, tile_key_max),
                    lod: 0,
                    note_count: pixel_data.note_count,
                };
                pool_guard.set_metadata(pool_idx, meta);
                tile_cache.insert(id, TileCacheEntry { pool_index: pool_idx, meta });

                // 生成瓦片引用（NDC 坐标，track_index 记录 pool 索引）
                let (pos, size) = tile_to_ndc(tile_tick_start, tile_tick_end, tile_key_min, tile_key_max);
                tile_refs.push(OnionBgTileRef {
                    position: pos,
                    size,
                    track_index: pool_idx as u32,
                    _padding: 0,
                });
            }

            // 调试日志：瓦片生成统计（仅 tile_count > 0 时输出 info）
            if !tile_refs.is_empty() {
                tracing::info!(
                    "[CK1] tiles generated: count={}, cache_hits={}, cache_misses={}",
                    tile_refs.len(), cache_hits, cache_misses,
                );
                for (i, tr) in tile_refs.iter().enumerate().take(2) {
                    tracing::info!(
                        "[CK1] tile[{}]: NDC pos=({:.2},{:.2}) size=({:.2},{:.2}) idx={}",
                        i, tr.position[0], tr.position[1], tr.size[0], tr.size[1], tr.track_index,
                    );
                }
            }

            // 写入 onion_bg_tiles_buffer
            {
                let buffer = unsafe { job.onion_bg_tiles_buffer.write_buffer() };
                buffer.clear();
                buffer.reserve(tile_refs.len());
                buffer.extend(tile_refs);
            }
            let ver = job.onion_bg_tiles_buffer.swap();
            tracing::info!("[CK1] onion_bg_tiles_buffer.swap() done, version={}", ver);

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
