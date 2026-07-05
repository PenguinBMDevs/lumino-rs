//! 高精度贴图并行生成调度
//!
//! 按**音轨组**维度 rayon 并行，组内顺序生成 8 轨 × N 时间组的单音轨贴图，
//! 缓存命中则跳过计算，组内全部就绪后合并为整合组贴图。
//!
//! 进度回调线程安全，多线程并发上报（与项目现有 ProgressManager 行为一致）。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use tracing::{info, warn};

use crate::cache;
use crate::config::HiResConfig;
use crate::scheduler::generate::{
    CacheWriteJob, TileGenContext, generate_one_time_group_tile, generate_one_track_group,
    sort_notes_per_track,
};
use crate::types::{GroupTile, TileCoord};
use lumino_onion_skin::OnionSkinNote;

mod generate;

/// 进度回调（线程安全，f32 百分比 0.0~1.0）
pub type HiResProgressCallback = Arc<dyn Fn(&str, f32) + Send + Sync>;

/// 生成错误
#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error("缓存 IO 错误: {0}")]
    CacheIo(String),
}

/// 生成全曲高精度贴图（rayon 并行）
///
/// # 参数
/// - `notes`: per-track 音符列表（外层 Vec 索引 = track_idx），函数会原地按 `start_ms` 排序
/// - `config`: 运行时配置
/// - `ppq`: MIDI ppq
/// - `key_count`: key 数量（128 或 256，= 贴图高度）
/// - `total_ticks`: 全曲总 tick
/// - `midi_hash`: MIDI 内容哈希（缓存分桶）
/// - `progress_cb`: 可选进度回调
///
/// 返回 `TileCoord → GroupTile` 的 HashMap，调用方负责管理内存缓冲与 GPU 上传。
pub fn generate_all_tiles(
    notes: &mut [Vec<OnionSkinNote>],
    config: &HiResConfig,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
    midi_hash: &str,
    progress_cb: Option<HiResProgressCallback>,
) -> HashMap<TileCoord, GroupTile> {
    sort_notes_per_track(notes);
    let track_count = notes.len() as u16;
    let track_groups = config.track_group_count(track_count);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let ticks_per_group = config.ticks_per_group(ppq);
    let total_tiles = (track_groups as usize) * (time_groups as usize);

    if total_tiles == 0 {
        if let Some(cb) = &progress_cb {
            cb("高精度贴图：无内容需生成", 1.0);
        }
        return HashMap::new();
    }

    info!(
        "高精度贴图生成开始：{} 轨 / {} 音轨组 × {} 时间组 = {} 贴图",
        track_count, track_groups, time_groups, total_tiles
    );

    let completed = Arc::new(AtomicUsize::new(0));
    let cache_dir = config.cache_dir.clone();
    let width = config.tile_width_px;
    let measures_per_group = config.measures_per_group;

    // ★ 缓存写入独立后台线程，避免 zstd+IO 阻塞 rayon 并行生成 ★
    // 有界 channel 背压：队列最多缓存 16 个 tile，防止无界堆积导致 OOM。
    const CACHE_BACKLOG: usize = 16;
    let (cache_tx, cache_rx) = std::sync::mpsc::sync_channel::<CacheWriteJob>(CACHE_BACKLOG);
    let cache_dir_for_thread = cache_dir.clone();
    let cache_handle = std::thread::spawn(move || {
        while let Ok(job) = cache_rx.recv() {
            if let Err(e) =
                cache::write_track_tile_cache(&job.cache_dir, &job.midi_hash, &job.tile, &job.meta)
            {
                warn!("缓存写入失败（不影响生成）: {e}");
            }
        }
        tracing::debug!(
            "高精度贴图缓存写入线程结束，目录: {:?}",
            cache_dir_for_thread
        );
    });

    let ctx = TileGenContext {
        ppq,
        key_count,
        width,
        measures_per_group,
        cache_dir: &cache_dir,
        midi_hash,
        cache_tx: &cache_tx,
    };

    // rayon 并行：音轨组维度（每组 8 轨）
    let group_results: Vec<Vec<(TileCoord, GroupTile)>> = (0..track_groups)
        .into_par_iter()
        .map(|track_group| {
            generate_one_track_group(
                track_group,
                notes,
                ticks_per_group,
                time_groups,
                &completed,
                total_tiles,
                &progress_cb,
                &ctx,
            )
        })
        .collect();

    // 关闭缓存发送端，等待后台线程把剩余缓存落盘
    drop(cache_tx);
    if let Err(e) = cache_handle.join() {
        warn!("缓存写入线程异常结束: {e:?}");
    }

    // 合并所有音轨组结果到单个 HashMap
    let mut buffer = HashMap::with_capacity(total_tiles);
    for tiles in group_results {
        for (coord, tile) in tiles {
            buffer.insert(coord, tile);
        }
    }

    if let Some(cb) = &progress_cb {
        cb("高精度贴图生成完成", 1.0);
    }

    info!("高精度贴图生成完成：{} 张整合组贴图", buffer.len());
    buffer
}

/// 流式生成全曲高精度贴图（time_group 同步推进模型）
///
/// 模型：所有音轨组线程并行生成**同一个 time_group** 的贴图，收齐后回调输出，
/// 调用方合并上传 GPU 并释放 CPU 缓冲，再进入下一个 time_group。
/// 避免按 track_group 维度独立并行导致的 channel 无界积压与 CPU 内存峰值。
///
/// # 参数
/// 除与 `generate_all_tiles` 相同的参数外：
/// - `time_group_cb`: 某个 time_group 的所有音轨组贴图收齐后回调，
///   参数为 `(time_group, Vec<GroupTile>)`，Vec 长度 = track_groups。
///   回调返回后 Vec 被 drop 释放 CPU 内存，才进入下一个 time_group。
pub fn generate_all_tiles_streaming<F>(
    notes: &mut [Vec<OnionSkinNote>],
    config: &HiResConfig,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
    midi_hash: &str,
    progress_cb: Option<HiResProgressCallback>,
    time_group_cb: &F,
) where
    F: Fn(u32, Vec<GroupTile>) + Sync,
{
    sort_notes_per_track(notes);
    let track_count = notes.len() as u16;
    let track_groups = config.track_group_count(track_count);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let ticks_per_group = config.ticks_per_group(ppq);
    let total_tiles = (track_groups as usize) * (time_groups as usize);

    if total_tiles == 0 {
        if let Some(cb) = &progress_cb {
            cb("高精度贴图：无内容需生成", 1.0);
        }
        return;
    }

    info!(
        "高精度贴图流式生成开始（time_group 同步推进）：{} 轨 / {} 音轨组 × {} 时间组 = {} 贴图",
        track_count, track_groups, time_groups, total_tiles
    );

    let cache_dir = config.cache_dir.clone();
    let width = config.tile_width_px;
    let measures_per_group = config.measures_per_group;
    let completed = Arc::new(AtomicUsize::new(0));

    // ★ 缓存写入独立后台线程，避免 zstd+IO 阻塞 rayon 并行生成 ★
    // 有界 channel 背压：队列最多缓存 16 个 tile，防止无界堆积导致 OOM。
    const CACHE_BACKLOG: usize = 16;
    let (cache_tx, cache_rx) = std::sync::mpsc::sync_channel::<CacheWriteJob>(CACHE_BACKLOG);
    let cache_dir_for_thread = cache_dir.clone();
    let cache_handle = std::thread::spawn(move || {
        while let Ok(job) = cache_rx.recv() {
            if let Err(e) =
                cache::write_track_tile_cache(&job.cache_dir, &job.midi_hash, &job.tile, &job.meta)
            {
                warn!("缓存写入失败（不影响生成）: {e}");
            }
        }
        tracing::debug!(
            "高精度贴图缓存写入线程结束，目录: {:?}",
            cache_dir_for_thread
        );
    });

    let ctx = TileGenContext {
        ppq,
        key_count,
        width,
        measures_per_group,
        cache_dir: &cache_dir,
        midi_hash,
        cache_tx: &cache_tx,
    };

    // ★ 外层串行 time_group，内层并行 track_group，rayon collect 天然 Barrier ★
    // 所有 track_group 线程完成同一个 time_group 后才回调，回调期间并行线程空闲
    // （对应"装袋期间工人等着"），回调返回后 Vec drop 释放，进入下一个 time_group
    for time_group in 0..time_groups {
        let tick_start = time_group * ticks_per_group;
        let tick_end = tick_start + ticks_per_group;

        // 所有音轨组并行生成这一个 time_group 的贴图，collect 等待全部完成
        let group_tiles: Vec<GroupTile> = (0..track_groups)
            .into_par_iter()
            .map(|track_group| {
                generate_one_time_group_tile(
                    track_group,
                    time_group,
                    tick_start,
                    tick_end,
                    notes,
                    &ctx,
                )
            })
            .collect();

        // ★ 所有 track_group 的 GroupTile 收齐，回调合并+上传+释放 ★
        time_group_cb(time_group, group_tiles);

        // 更新进度（按 time_group 粒度），原子计数替代 Mutex
        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(cb) = &progress_cb {
            let pct = done as f32 / time_groups as f32;
            cb(
                &format!("高精度贴图 time_group {}/{}", done, time_groups),
                pct,
            );
        }
    }

    // 关闭缓存发送端，等待后台线程把剩余缓存落盘
    drop(cache_tx);
    if let Err(e) = cache_handle.join() {
        warn!("缓存写入线程异常结束: {e:?}");
    }

    if let Some(cb) = &progress_cb {
        cb("高精度贴图流式生成完成", 1.0);
    }

    info!("高精度贴图流式生成完成：{} 个 time_group", time_groups);
}

#[cfg(test)]
mod tests;
