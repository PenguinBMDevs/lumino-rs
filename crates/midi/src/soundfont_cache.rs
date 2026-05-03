//! 音色库缓存模块
//!
//! 提供音色库的缓存功能，避免重复加载相同的音色库
//! 缓存 key = (文件路径, 采样率)，因为 sample_rate 影响 soundfont 内部预处理

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use xsynth_core::AudioStreamParams;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};

/// 音色库缓存 key: (文件路径, 采样率)
type CacheKey = (PathBuf, u32);

/// 音色库缓存
static SOUNDFONT_CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

/// 缓存条目
struct CacheEntry {
    soundfont: Arc<dyn SoundfontBase>,
    modified: std::time::SystemTime,
}

/// 获取全局缓存
fn get_cache() -> &'static Mutex<HashMap<CacheKey, CacheEntry>> {
    SOUNDFONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 从缓存获取音色库
fn get_from_cache(path: &Path, sample_rate: u32) -> Option<Arc<dyn SoundfontBase>> {
    let cache = get_cache().lock().ok()?;
    let key = (path.to_path_buf(), sample_rate);

    if let Some(entry) = cache.get(&key)
        && let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
        && modified == entry.modified
    {
        return Some(entry.soundfont.clone());
    }
    None
}

/// 将音色库添加到缓存
fn add_to_cache(path: &Path, sample_rate: u32, soundfont: Arc<dyn SoundfontBase>) {
    if let Ok(mut cache) = get_cache().lock()
        && let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
    {
        let key = (path.to_path_buf(), sample_rate);
        cache.insert(
            key,
            CacheEntry {
                soundfont,
                modified,
            },
        );
    }
}

/// 清空缓存（配置变更时调用，防止旧条目无限累积）
///
/// SoundFont 通常 30-300MB 每个，不清空会导致永不下沉的"气球"。
pub fn clear_cache() {
    if let Ok(mut cache) = get_cache().lock() {
        let count = cache.len();
        cache.clear();
        tracing::info!("SoundfontCache: 已清空 {} 个缓存条目", count);
    }
}

/// 从文件加载音色库（带缓存）
///
/// 缓存 key 包含文件路径和采样率两个维度，
/// 确保不同采样率下不会复用到不匹配的 soundfont 实例。
pub fn load_soundfont_cached(
    path: &Path,
    params: AudioStreamParams,
) -> Result<Arc<dyn SoundfontBase>, String> {
    let sample_rate = params.sample_rate;

    // 先尝试从缓存获取
    if let Some(cached) = get_from_cache(path, sample_rate) {
        tracing::info!("SoundfontCache: 命中缓存 {:?} (sr={})", path, sample_rate);
        return Ok(cached);
    }

    // 缓存未命中，加载音色库
<<<<<<< HEAD
    tracing::info!("SoundfontCache: 缓存未命中，加载音色库 {:?} (sr={})", path, sample_rate);
=======
    tracing::info!(
        "SoundfontCache: 缓存未命中，加载音色库 {:?} (sr={})",
        path,
        sample_rate
    );
>>>>>>> feat/memory-for-loader
    let soundfont = SampleSoundfont::new(path, params, Default::default())
        .map_err(|e| format!("Failed to load soundfont: {:?}", e))?;

    let soundfont: Arc<dyn SoundfontBase> = Arc::new(soundfont);

    // 添加到缓存
    add_to_cache(path, sample_rate, soundfont.clone());

    Ok(soundfont)
}
