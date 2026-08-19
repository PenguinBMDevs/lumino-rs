//! 贴图瀑布流并行生成调度
//!
//! 提供两个生成模式：
//! - `generate_waterfall_tiles`：按音轨组 rayon 并行，返回完整 HashMap，适合小 MIDI。
//! - `generate_waterfall_tiles_streaming`：按 time_group 串行推进，每生成一张整合组贴图
//!   立即回调上传，不累积 Vec，适合大 MIDI 低内存峰值场景。
//!
//! 进度回调线程安全，多线程并发上报（与项目现有 ProgressManager 行为一致）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use tracing::{info, warn};

use crate::texture_waterfall::cache;
use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::note::WaterfallNote;
use crate::texture_waterfall::scheduler::generate::{
    CacheWriteJob, TileGenContext, TrackGroupRequest, generate_one_time_group_tile_into,
    generate_one_track_group, sort_notes_per_track,
};
use crate::texture_waterfall::types::{WaterfallGroupTile, WaterfallTileCoord};
use lumino_diagnostics::memory_monitor::MemoryMonitor;

mod generate;

/// 进度回调（线程安全，f32 百分比 0.0~1.0）
pub type TextureWaterfallProgressCallback = Arc<dyn Fn(&str, f32) + Send + Sync>;

/// 生成错误
#[derive(Debug, thiserror::Error)]
pub enum WaterfallGenerateError {
    /// 缓存 IO 错误
    #[error("缓存 IO 错误: {0}")]
    CacheIo(String),
}

/// 流式贴图瀑布流生成配置参数
///
/// 聚合 `generate_waterfall_tiles_streaming` 中不随回调变化的静态配置，
/// 将函数签名降到 7 个参数以下，同时保持 `progress_cb` 与 `time_group_cb` 独立。
#[derive(Debug)]
pub struct WaterfallGenContext<'a> {
    /// 贴图瀑布流运行配置
    pub config: &'a TextureWaterfallConfig,
    /// 每四分音符 tick 数
    pub ppq: u16,
    /// 键盘数量
    pub key_count: u16,
    /// 整首 MIDI 总 tick 数
    pub total_ticks: u32,
    /// MIDI 内容哈希（用于缓存失效校验）
    pub midi_hash: &'a str,
}

/// 贴图瀑布流缓存写入线程 + 共享上下文创建。
///
/// 创建一个独立后台线程从 `cache_rx` 接收 `CacheWriteJob` 并逐条写入硬盘，
/// 避免 zstd 压缩 + 文件 I/O 阻塞主生成路径。
/// 有界 channel（16）提供背压，防止无界堆积导致 OOM。
///
/// 返回 `(cache_tx, cache_handle)`，调用方应在生成完成后 `drop(cache_tx)`
/// 再 `join` 句柄等待剩余缓存落盘。
fn spawn_cache_writer(
    cache_dir: PathBuf,
) -> (
    std::sync::mpsc::SyncSender<CacheWriteJob>,
    std::thread::JoinHandle<()>,
) {
    const CACHE_BACKLOG: usize = 16;
    let (cache_tx, cache_rx) = std::sync::mpsc::sync_channel::<CacheWriteJob>(CACHE_BACKLOG);
    let cache_dir_for_thread = cache_dir.clone();
    let cache_handle = std::thread::spawn(move || {
        while let Ok(job) = cache_rx.recv() {
            if let Err(e) = cache::write_waterfall_track_tile_cache(
                &job.cache_dir,
                &job.midi_hash,
                &job.tile,
                &job.meta,
            ) {
                warn!("缓存写入失败（不影响生成）: {e}");
            }
        }
        tracing::debug!(
            "贴图瀑布流缓存写入线程结束，目录: {:?}",
            cache_dir_for_thread
        );
    });
    (cache_tx, cache_handle)
}

/// 关闭缓存发送端，等待后台线程把剩余缓存落盘。
fn join_cache_thread(
    cache_tx: std::sync::mpsc::SyncSender<CacheWriteJob>,
    cache_handle: std::thread::JoinHandle<()>,
) {
    drop(cache_tx);
    if let Err(e) = cache_handle.join() {
        warn!("缓存写入线程异常结束: {e:?}");
    }
}

/// 生成全曲贴图瀑布流（rayon 并行）
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
/// 返回 `WaterfallTileCoord → WaterfallGroupTile` 的 HashMap，调用方负责管理内存缓冲与 GPU 上传。
pub fn generate_waterfall_tiles(
    notes: &mut [Vec<WaterfallNote>],
    config: &TextureWaterfallConfig,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
    midi_hash: &str,
    progress_cb: Option<TextureWaterfallProgressCallback>,
) -> HashMap<WaterfallTileCoord, WaterfallGroupTile> {
    sort_notes_per_track(notes);
    let track_count = notes.len() as u16;
    let track_groups = config.track_group_count(track_count);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let ticks_per_group = config.ticks_per_group(ppq);
    let total_tiles = (track_groups as usize) * (time_groups as usize);

    if total_tiles == 0 {
        if let Some(cb) = &progress_cb {
            cb("贴图瀑布流：无内容需生成", 1.0);
        }
        return HashMap::new();
    }

    info!(
        "贴图瀑布流生成开始：{} 轨 / {} 音轨组 × {} 时间组 = {} 贴图",
        track_count, track_groups, time_groups, total_tiles
    );

    let completed = Arc::new(AtomicUsize::new(0));
    let cache_dir = config.cache_dir.clone();
    let width = config.tile_width_px;
    let measures_per_group = config.measures_per_group;

    // ★ 缓存写入独立后台线程 ★
    let (cache_tx, cache_handle) = spawn_cache_writer(cache_dir.clone());

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
    let group_results: Vec<Vec<(WaterfallTileCoord, WaterfallGroupTile)>> = (0..track_groups)
        .into_par_iter()
        .map(|track_group| {
            generate_one_track_group(
                &TrackGroupRequest {
                    track_group,
                    notes,
                    ticks_per_group,
                    time_groups,
                    completed: &completed,
                    total_tiles,
                    progress_cb: &progress_cb,
                },
                &ctx,
            )
        })
        .collect();

    // 关闭缓存发送端，等待后台线程把剩余缓存落盘
    join_cache_thread(cache_tx, cache_handle);

    // 合并所有音轨组结果到单个 HashMap
    let mut buffer = HashMap::with_capacity(total_tiles);
    for tiles in group_results {
        for (coord, tile) in tiles {
            buffer.insert(coord, tile);
        }
    }

    if let Some(cb) = &progress_cb {
        cb("贴图瀑布流生成完成", 1.0);
    }

    info!("贴图瀑布流生成完成：{} 张整合组贴图", buffer.len());
    buffer
}

/// 流式生成全曲贴图瀑布流（单 tile 流式回调模型）
///
/// 模型：按 time_group 串行推进，每个 track_group 的整合组贴图一生成完毕
/// 立即通过回调输出，调用方直接上传 GPU 并释放 CPU 缓冲，再生成下一张。
/// 避免一个 time_group 的所有贴图在内存中累积成 Vec 后再统一上传。
///
/// # 参数
/// 除与 `generate_waterfall_tiles` 相同的参数外：
/// - `time_group_cb`: 每生成一张整合组贴图立即回调，参数为 `(time_group, WaterfallGroupTile)`。
///   回调返回后该贴图的 CPU 像素缓冲即可释放，才继续生成下一张。
pub fn generate_waterfall_tiles_streaming<F>(
    notes: &mut [Vec<WaterfallNote>],
    ctx: &WaterfallGenContext<'_>,
    progress_cb: Option<TextureWaterfallProgressCallback>,
    time_group_cb: &F,
) where
    F: Fn(u32, WaterfallGroupTile) + Sync,
{
    sort_notes_per_track(notes);
    let track_count = notes.len() as u16;
    let track_groups = ctx.config.track_group_count(track_count);
    let time_groups = ctx.config.time_group_count(ctx.total_ticks, ctx.ppq);
    let ticks_per_group = ctx.config.ticks_per_group(ctx.ppq);
    let total_tiles = (track_groups as usize) * (time_groups as usize);

    if total_tiles == 0 {
        if let Some(cb) = &progress_cb {
            cb("贴图瀑布流：无内容需生成", 1.0);
        }
        return;
    }

    info!(
        "贴图瀑布流流式生成开始（time_group 同步推进）：{} 轨 / {} 音轨组 × {} 时间组 = {} 贴图",
        track_count, track_groups, time_groups, total_tiles
    );

    let cache_dir = ctx.config.cache_dir.clone();
    let width = ctx.config.tile_width_px;
    let measures_per_group = ctx.config.measures_per_group;
    let completed = Arc::new(AtomicUsize::new(0));

    // ★ 缓存写入独立后台线程 ★
    let (cache_tx, cache_handle) = spawn_cache_writer(cache_dir.clone());

    let tile_ctx = TileGenContext {
        ppq: ctx.ppq,
        key_count: ctx.key_count,
        width,
        measures_per_group,
        cache_dir: &cache_dir,
        midi_hash: ctx.midi_hash,
        cache_tx: &cache_tx,
    };

    // ★ 跨 track_group 合并：一个 time_group 内所有 track_group 的 WaterfallGroupTile 合并为一张 ★
    // 避免 104 × 101 = 10504 张零散贴图塞进 GPU 显存。
    // GPU 最终只持有 time_groups 张合并贴图（用户预期：~101 张而非 10504 张）。
    for time_group in 0..time_groups {
        // 大分配前主动检查内存，接近上限时提前 panic，避免 OOM 把系统拖死
        MemoryMonitor::global().check();

        let tick_start = time_group * ticks_per_group;
        let tick_end = tick_start + ticks_per_group;

        let buf_size = (width * ctx.key_count as u32) as usize * 4;
        let mut merged_pixels = vec![0u8; buf_size];

        for track_group in 0..track_groups {
            // 直接写入 merged_pixels，不创建中间 WaterfallGroupTile，省去一次完整贴图分配
            generate_one_time_group_tile_into(
                track_group,
                time_group,
                tick_start,
                tick_end,
                notes,
                &tile_ctx,
                &mut merged_pixels,
            );
        }

        let merged = WaterfallGroupTile {
            coord: WaterfallTileCoord::new(0, time_group),
            pixels: merged_pixels,
            width,
            height: ctx.key_count as u32,
            tick_start,
            tick_end,
            track_range: (0, track_count),
        };
        time_group_cb(time_group, merged);

        // 更新进度（按 time_group 粒度），原子计数替代 Mutex
        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(cb) = &progress_cb {
            let pct = done as f32 / time_groups as f32;
            cb(
                &format!("贴图瀑布流 time_group {}/{}", done, time_groups),
                pct,
            );
        }
    }

    // 关闭缓存发送端，等待后台线程把剩余缓存落盘
    join_cache_thread(cache_tx, cache_handle);

    if let Some(cb) = &progress_cb {
        cb("贴图瀑布流流式生成完成", 1.0);
    }

    info!("贴图瀑布流流式生成完成：{} 个 time_group", time_groups);
}

#[cfg(test)]
mod tests;
