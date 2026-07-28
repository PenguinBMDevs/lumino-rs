//! 单音轨贴图生成与缓存加载辅助
//!
//! 把 `generate_or_load_track_tile` 及其依赖从 `scheduler.rs` 拆出，
//! 避免调度文件过长，同时保持缓存写入异步化逻辑内聚。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;

use tracing::warn;

use crate::cache::{self, CacheMeta, cache_path};
use crate::generate::{generate_track_tile, merge_group_tiles, merge_track_tile_into};
use crate::scheduler::HiResProgressCallback;
use crate::types::{GroupTile, TileCoord, TrackTile};
use lumino_onion_skin::OnionSkinNote;

/// 待写入硬盘的单音轨缓存任务
pub(super) struct CacheWriteJob {
    pub(super) cache_dir: PathBuf,
    pub(super) midi_hash: String,
    pub(super) tile: TrackTile,
    pub(super) meta: CacheMeta,
}

/// 贴图生成共享上下文（替代 7 个重复参数）
///
/// 将 `ppq`、`key_count`、`width`、`measures_per_group`、`cache_dir`、
/// `midi_hash`、`cache_tx` 聚合为单一结构体，减少函数签名膨胀。
pub(super) struct TileGenContext<'a> {
    pub ppq: u16,
    pub key_count: u16,
    pub width: u32,
    pub measures_per_group: u32,
    pub cache_dir: &'a Path,
    pub midi_hash: &'a str,
    pub cache_tx: &'a SyncSender<CacheWriteJob>,
}

/// 单音轨组生成请求参数
///
/// 聚合 `generate_one_track_group` 中随每次调用变化的参数，
/// 与共享的 `TileGenContext` 配合，将函数签名降到 7 个参数以下。
pub(super) struct TrackGroupRequest<'a> {
    pub track_group: u32,
    pub notes: &'a [Vec<OnionSkinNote>],
    pub ticks_per_group: u32,
    pub time_groups: u32,
    pub completed: &'a Arc<AtomicUsize>,
    pub total_tiles: usize,
    pub progress_cb: &'a Option<HiResProgressCallback>,
}

/// 对每轨音符按 `start_ms` 升序排序，供后续二分剪枝使用。
pub(super) fn sort_notes_per_track(notes: &mut [Vec<OnionSkinNote>]) {
    for track in notes.iter_mut() {
        // total_cmp 对 NaN 也按 IEEE 754 totalOrder 给出稳定顺序，无需 unwrap
        track.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    }
}

/// 生成单个音轨组在指定 time_group 的整合组贴图
///
/// 内部：逐轨生成单音轨贴图（缓存优先），边生成边合并到整合组缓冲，
/// 合并后立即释放单轨贴图，避免 8 张完整尺寸贴图同时堆积在内存中。
pub(super) fn generate_one_time_group_tile(
    track_group: u32,
    time_group: u32,
    tick_start: u32,
    tick_end: u32,
    notes: &[Vec<OnionSkinNote>],
    ctx: &TileGenContext<'_>,
) -> GroupTile {
    let track_start = (track_group * crate::config::TRACKS_PER_GROUP as u32) as u16;
    let track_end =
        ((track_group + 1) * crate::config::TRACKS_PER_GROUP as u32).min(notes.len() as u32) as u16;

    let coord = TileCoord::new(track_group, time_group);
    let pixel_count = (ctx.width * ctx.key_count as u32) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    // ★ streaming merge：生成一轨、合并一轨、释放一轨 ★
    // 避免 Vec<TrackTile> 同时持有 8 张完整尺寸贴图导致的内存峰值。
    for track_idx in track_start..track_end {
        let tile = generate_or_load_track_tile(
            &notes[track_idx as usize],
            track_idx,
            time_group,
            tick_start,
            tick_end,
            ctx,
        );
        merge_track_tile_into(&mut pixels, &tile);
        // tile 在此作用域结束时 drop，CPU 像素缓冲立即释放
    }

    GroupTile {
        coord,
        pixels,
        width: ctx.width,
        height: ctx.key_count as u32,
        tick_start,
        tick_end,
        track_range: (track_start, track_end),
    }
}

/// 生成单个音轨组的所有时间组贴图
pub(super) fn generate_one_track_group(
    req: &TrackGroupRequest<'_>,
    ctx: &TileGenContext<'_>,
) -> Vec<(TileCoord, GroupTile)> {
    let track_start = (req.track_group * crate::config::TRACKS_PER_GROUP as u32) as u16;
    let track_end = ((req.track_group + 1) * crate::config::TRACKS_PER_GROUP as u32)
        .min(req.notes.len() as u32) as u16;
    let mut group_tiles = Vec::with_capacity(req.time_groups as usize);

    for time_group in 0..req.time_groups {
        let tick_start = time_group * req.ticks_per_group;
        let tick_end = tick_start + req.ticks_per_group;

        // 生成组内每轨的单音轨贴图（缓存优先）
        let mut track_tiles = Vec::with_capacity((track_end - track_start) as usize);
        for track_idx in track_start..track_end {
            let tile = generate_or_load_track_tile(
                &req.notes[track_idx as usize],
                track_idx,
                time_group,
                tick_start,
                tick_end,
                ctx,
            );
            track_tiles.push(tile);
        }

        // 合并为整合组贴图（后轨覆盖前轨重叠区）
        let coord = TileCoord::new(req.track_group, time_group);
        let group_tile = merge_group_tiles(
            &track_tiles,
            coord,
            tick_start,
            tick_end,
            ctx.width,
            ctx.key_count,
            (track_start, track_end),
        );
        group_tiles.push((coord, group_tile));

        // 更新进度，原子计数替代 Mutex
        let done = req.completed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(cb) = req.progress_cb {
            let pct = done as f32 / req.total_tiles as f32;
            cb(&format!("高精度贴图 {}/{}", done, req.total_tiles), pct);
        }
    }

    group_tiles
}

/// 生成或从缓存加载单音轨贴图
pub(super) fn generate_or_load_track_tile(
    notes: &[OnionSkinNote],
    track_idx: u16,
    time_group: u32,
    tick_start: u32,
    tick_end: u32,
    ctx: &TileGenContext<'_>,
) -> TrackTile {
    let expected_meta = CacheMeta {
        track_idx,
        time_group,
        width: ctx.width,
        height: ctx.key_count as u32,
        tick_start,
        tick_end,
        key_count: ctx.key_count,
        ppq: ctx.ppq,
        measures_per_group: ctx.measures_per_group,
    };

    // 先查缓存
    match cache::read_track_tile_cache(
        ctx.cache_dir,
        ctx.midi_hash,
        track_idx,
        time_group,
        &expected_meta,
    ) {
        Ok(Some(tile)) => return tile, // 缓存命中
        Ok(None) => {}                 // 缓存未命中，生成
        Err(e) => {
            warn!("缓存读取失败（将重生成）: {e}");
            let path = cache_path(ctx.cache_dir, ctx.midi_hash, track_idx, time_group);
            let _ = std::fs::remove_file(path);
        }
    }

    // 生成单音轨贴图
    let tile = generate_track_tile(
        notes,
        track_idx,
        time_group,
        tick_start,
        tick_end,
        ctx.width,
        ctx.key_count,
    );

    // 写缓存入队，后台线程执行 zstd+IO，避免阻塞 rayon 并行生成
    // 使用有界 channel + try_send：队列满时直接丢弃缓存任务，避免无界堆积导致 OOM。
    // 缓存是性能优化，跳过不影响生成正确性。
    //
    // ★ TrackTile.pixels 已改为 Arc<Vec<u8>>，tile.clone() 仅增加引用计数，
    // 不再复制整张贴图像素，显著降低大 MIDI 场景下缓存队列的内存峰值。
    match ctx.cache_tx.try_send(CacheWriteJob {
        cache_dir: ctx.cache_dir.to_path_buf(),
        midi_hash: ctx.midi_hash.to_string(),
        tile: tile.clone(),
        meta: expected_meta,
    }) {
        Ok(()) => {}
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            tracing::debug!("缓存写入队列已满，跳过本次缓存以避免内存无界堆积");
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(job)) => {
            warn!("缓存写入通道已关闭，退化同步写入");
            if let Err(e) =
                cache::write_track_tile_cache(&job.cache_dir, &job.midi_hash, &job.tile, &job.meta)
            {
                warn!("缓存同步写入失败（不影响生成）: {e}");
            }
        }
    }

    tile
}
