//! 高精度贴图并行生成调度
//!
//! 按**音轨组**维度 rayon 并行，组内顺序生成 8 轨 × N 时间组的单音轨贴图，
//! 缓存命中则跳过计算，组内全部就绪后合并为整合组贴图。
//!
//! 进度回调线程安全，多线程并发上报（与项目现有 ProgressManager 行为一致）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use tracing::{info, warn};

use crate::cache::{self, CacheMeta, cache_path};
use crate::config::HiResConfig;
use crate::generate::{generate_track_tile, merge_group_tiles};
use crate::types::{GroupTile, TileCoord, TrackTile};
use lumino_onion_skin::OnionSkinNote;

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
/// - `notes`: per-track 音符列表（外层 Vec 索引 = track_idx）
/// - `config`: 运行时配置
/// - `ppq`: MIDI ppq
/// - `key_count`: key 数量（128 或 256，= 贴图高度）
/// - `total_ticks`: 全曲总 tick
/// - `midi_hash`: MIDI 内容哈希（缓存分桶）
/// - `progress_cb`: 可选进度回调
///
/// 返回 `TileCoord → GroupTile` 的 HashMap，调用方负责管理内存缓冲与 GPU 上传。
pub fn generate_all_tiles(
    notes: &[Vec<OnionSkinNote>],
    config: &HiResConfig,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
    midi_hash: &str,
    progress_cb: Option<HiResProgressCallback>,
) -> HashMap<TileCoord, GroupTile> {
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

    let completed = Arc::new(Mutex::new(0usize));
    let cache_dir = config.cache_dir.clone();
    let width = config.tile_width_px;
    let measures_per_group = config.measures_per_group;

    // rayon 并行：音轨组维度（每组 8 轨）
    let group_results: Vec<Vec<(TileCoord, GroupTile)>> = (0..track_groups)
        .into_par_iter()
        .map(|track_group| {
            generate_one_track_group(
                track_group,
                notes,
                ppq,
                key_count,
                ticks_per_group,
                time_groups,
                &cache_dir,
                midi_hash,
                width,
                measures_per_group,
                &completed,
                total_tiles,
                &progress_cb,
            )
        })
        .collect();

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

/// 生成单个音轨组的所有时间组贴图
#[allow(clippy::too_many_arguments)]
fn generate_one_track_group(
    track_group: u32,
    notes: &[Vec<OnionSkinNote>],
    ppq: u16,
    key_count: u16,
    ticks_per_group: u32,
    time_groups: u32,
    cache_dir: &Path,
    midi_hash: &str,
    width: u32,
    measures_per_group: u32,
    completed: &Arc<Mutex<usize>>,
    total_tiles: usize,
    progress_cb: &Option<HiResProgressCallback>,
) -> Vec<(TileCoord, GroupTile)> {
    let track_start = (track_group * crate::config::TRACKS_PER_GROUP as u32) as u16;
    let track_end =
        ((track_group + 1) * crate::config::TRACKS_PER_GROUP as u32).min(notes.len() as u32) as u16;
    let mut group_tiles = Vec::with_capacity(time_groups as usize);

    for time_group in 0..time_groups {
        let tick_start = time_group * ticks_per_group;
        let tick_end = tick_start + ticks_per_group;

        // 生成组内每轨的单音轨贴图（缓存优先）
        let mut track_tiles = Vec::with_capacity((track_end - track_start) as usize);
        for track_idx in track_start..track_end {
            let tile = generate_or_load_track_tile(
                &notes[track_idx as usize],
                track_idx,
                time_group,
                tick_start,
                tick_end,
                width,
                key_count,
                ppq,
                measures_per_group,
                cache_dir,
                midi_hash,
            );
            track_tiles.push(tile);
        }

        // 合并为整合组贴图（后轨覆盖前轨重叠区）
        let coord = TileCoord::new(track_group, time_group);
        let group_tile = merge_group_tiles(
            &track_tiles,
            coord,
            tick_start,
            tick_end,
            width,
            key_count,
            (track_start, track_end),
        );
        group_tiles.push((coord, group_tile));

        // 更新进度
        let mut done = completed.lock().expect("进度锁中毒");
        *done += 1;
        if let Some(cb) = progress_cb {
            let pct = *done as f32 / total_tiles as f32;
            cb(&format!("高精度贴图 {}/{}", *done, total_tiles), pct);
        }
    }

    group_tiles
}

/// 生成或从缓存加载单音轨贴图
#[allow(clippy::too_many_arguments)]
fn generate_or_load_track_tile(
    notes: &[OnionSkinNote],
    track_idx: u16,
    time_group: u32,
    tick_start: u32,
    tick_end: u32,
    width: u32,
    key_count: u16,
    ppq: u16,
    measures_per_group: u32,
    cache_dir: &Path,
    midi_hash: &str,
) -> TrackTile {
    let expected_meta = CacheMeta {
        track_idx,
        time_group,
        width,
        height: key_count as u32,
        tick_start,
        tick_end,
        key_count,
        ppq,
        measures_per_group,
    };

    // 先查缓存
    match cache::read_track_tile_cache(cache_dir, midi_hash, track_idx, time_group, &expected_meta)
    {
        Ok(Some(tile)) => return tile, // 缓存命中
        Ok(None) => {}                 // 缓存未命中，生成
        Err(e) => {
            warn!("缓存读取失败（将重生成）: {e}");
            let path = cache_path(cache_dir, midi_hash, track_idx, time_group);
            let _ = std::fs::remove_file(path);
        }
    }

    // 生成单音轨贴图
    let tile = generate_track_tile(
        notes, track_idx, time_group, tick_start, tick_end, width, key_count,
    );

    // 写缓存（失败不阻塞生成流程）
    if let Err(e) = cache::write_track_tile_cache(cache_dir, midi_hash, &tile, &expected_meta) {
        warn!("缓存写入失败（不影响生成）: {e}");
    }

    tile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute_midi_hash;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn note(start: u32, end: u32, key: u8, color: [u8; 4]) -> OnionSkinNote {
        OnionSkinNote::from_ms(start as f32, end as f32, key, color)
    }

    fn test_config() -> (HiResConfig, String) {
        let mut config = HiResConfig::default();
        let dir = std::env::temp_dir()
            .join("lumino-hires-sched-test")
            .join(format!("{}-{}", std::process::id(), unique_id()));
        let _ = std::fs::remove_dir_all(&dir);
        config.cache_dir = dir.clone();
        (config, compute_midi_hash(b"sched-test"))
    }

    fn unique_id() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    fn cleanup(config: &HiResConfig) {
        let _ = std::fs::remove_dir_all(&config.cache_dir);
    }

    fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        ]
    }

    #[test]
    fn test_generate_empty_notes() {
        let (config, hash) = test_config();
        let result = generate_all_tiles(&[], &config, 1920, 128, 30720, &hash, None);
        assert!(result.is_empty());
        cleanup(&config);
    }

    #[test]
    fn test_generate_empty_ticks() {
        let (config, hash) = test_config();
        let notes = vec![vec![note(0, 100, 60, [255, 0, 0, 255])]];
        let result = generate_all_tiles(&notes, &config, 1920, 128, 0, &hash, None);
        assert!(result.is_empty());
        cleanup(&config);
    }

    #[test]
    fn test_generate_single_group() {
        // 3 轨，1 个时间组（total_ticks=30720）
        let (config, hash) = test_config();
        let notes = vec![
            vec![note(0, 15360, 60, [255, 0, 0, 255])],
            vec![note(0, 15360, 61, [0, 255, 0, 255])],
            vec![note(15360, 30720, 60, [0, 0, 255, 255])],
        ];

        let result = generate_all_tiles(&notes, &config, 1920, 128, 30720, &hash, None);

        // 1 音轨组 × 1 时间组 = 1 贴图
        assert_eq!(result.len(), 1);
        let coord = TileCoord::new(0, 0);
        let tile = result.get(&coord).expect("应有 (0,0) 贴图");
        assert_eq!(tile.track_range, (0, 3));
        assert_eq!(tile.track_count(), 3);

        // key=60 左半红，右半蓝（track2 覆盖 track0）
        assert_eq!(pixel_at(&tile.pixels, 1920, 0, 60), [255, 0, 0, 255]);
        assert_eq!(pixel_at(&tile.pixels, 1920, 1000, 60), [0, 0, 255, 255]);
        // key=61 左半绿
        assert_eq!(pixel_at(&tile.pixels, 1920, 0, 61), [0, 255, 0, 255]);

        cleanup(&config);
    }

    #[test]
    fn test_generate_multi_track_groups() {
        // 10 轨 → 2 音轨组（8+2），1 时间组
        let (config, hash) = test_config();
        let notes: Vec<Vec<OnionSkinNote>> = (0..10)
            .map(|i| vec![note(0, 100, i, [i, 0, 0, 255])])
            .collect();

        let result = generate_all_tiles(&notes, &config, 1920, 128, 30720, &hash, None);

        // 2 音轨组 × 1 时间组 = 2 贴图
        assert_eq!(result.len(), 2);
        let g0 = result.get(&TileCoord::new(0, 0)).expect("音轨组0");
        let g1 = result.get(&TileCoord::new(1, 0)).expect("音轨组1");
        assert_eq!(g0.track_range, (0, 8));
        assert_eq!(g1.track_range, (8, 10));
        assert_eq!(g0.track_count(), 8);
        assert_eq!(g1.track_count(), 2);

        cleanup(&config);
    }

    #[test]
    fn test_generate_multi_time_groups() {
        // 1 轨，2 时间组（total_ticks=61440 = 2×30720）
        let (config, hash) = test_config();
        let notes = vec![vec![
            note(0, 15360, 60, [255, 0, 0, 255]),     // 组0
            note(40000, 50000, 64, [0, 0, 255, 255]), // 组1
        ]];

        let result = generate_all_tiles(&notes, &config, 1920, 128, 61440, &hash, None);

        // 1 音轨组 × 2 时间组 = 2 贴图
        assert_eq!(result.len(), 2);
        let g0 = result.get(&TileCoord::new(0, 0)).expect("时间组0");
        let g1 = result.get(&TileCoord::new(0, 1)).expect("时间组1");

        // 组0 key=60 有红色
        assert_eq!(pixel_at(&g0.pixels, 1920, 0, 60), [255, 0, 0, 255]);
        // 组1 key=64 有蓝色（音符 40000 在组1 内，偏移 9280 tick → x≈580）
        let x_in_g1 = ((40000u32 - 30720) as f32 / 30720.0 * 1920.0) as u32;
        assert_eq!(pixel_at(&g1.pixels, 1920, x_in_g1, 64), [0, 0, 255, 255]);

        cleanup(&config);
    }

    #[test]
    fn test_cache_hit_skips_generation() {
        // 第一次生成写缓存，第二次生成应从缓存读（像素一致）
        let (config, hash) = test_config();
        let notes = vec![vec![note(0, 15360, 60, [255, 0, 0, 255])]];

        let first = generate_all_tiles(&notes, &config, 1920, 128, 30720, &hash, None);
        let second = generate_all_tiles(&notes, &config, 1920, 128, 30720, &hash, None);

        let t1 = first.get(&TileCoord::new(0, 0)).unwrap();
        let t2 = second.get(&TileCoord::new(0, 0)).unwrap();
        assert_eq!(t1.pixels, t2.pixels, "缓存命中应产生相同像素");

        cleanup(&config);
    }

    #[test]
    fn test_progress_callback_invoked() {
        let (config, hash) = test_config();
        let notes = vec![
            vec![note(0, 100, 60, [255, 0, 0, 255])],
            vec![note(0, 100, 61, [0, 255, 0, 255])],
        ];

        let call_count = Arc::new(AtomicUsize::new(0));
        let final_pct = Arc::new(Mutex::new(0.0f32));
        let cb_count = call_count.clone();
        let cb_pct = final_pct.clone();
        let cb: HiResProgressCallback = Arc::new(move |_msg, pct| {
            cb_count.fetch_add(1, Ordering::SeqCst);
            *cb_pct.lock().unwrap() = pct;
        });

        let result = generate_all_tiles(&notes, &config, 1920, 128, 30720, &hash, Some(cb));

        assert_eq!(result.len(), 1);
        // 至少调用：1 次进度 + 1 次完成
        assert!(call_count.load(Ordering::SeqCst) >= 2);
        // 最终 pct 应为 1.0
        let pct = *final_pct.lock().unwrap();
        assert!((pct - 1.0).abs() < 0.001, "最终进度应为 1.0，实际 {pct}");

        cleanup(&config);
    }
}
