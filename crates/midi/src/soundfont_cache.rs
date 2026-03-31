//! 音色库缓存模块
//!
//! 提供音色库的缓存功能，避免重复加载相同的音色库

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use xsynth_core::AudioStreamParams;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};

/// 音色库缓存
static SOUNDFONT_CACHE: OnceLock<Mutex<HashMap<PathBuf, CacheEntry>>> = OnceLock::new();

/// 缓存条目
struct CacheEntry {
    soundfont: Arc<dyn SoundfontBase>,
    modified: std::time::SystemTime,
}

/// 获取全局缓存
fn get_cache() -> &'static Mutex<HashMap<PathBuf, CacheEntry>> {
    SOUNDFONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 从缓存获取音色库
pub fn get_from_cache(path: &Path) -> Option<Arc<dyn SoundfontBase>> {
    let cache = get_cache().lock().ok()?;

    if let Some(entry) = cache.get(path)
        && let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
        && modified == entry.modified
    {
        return Some(entry.soundfont.clone());
    }
    None
}

/// 将音色库添加到缓存
pub fn add_to_cache(path: &Path, soundfont: Arc<dyn SoundfontBase>) {
    if let Ok(mut cache) = get_cache().lock()
        && let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
    {
        cache.insert(
            path.to_path_buf(),
            CacheEntry {
                soundfont,
                modified,
            },
        );
    }
}

/// 从文件加载音色库（带缓存）
///
/// 如果缓存中有该音色库，直接返回；否则加载并缓存
pub fn load_soundfont_cached(
    path: &Path,
    params: AudioStreamParams,
) -> Result<Arc<dyn SoundfontBase>, String> {
    // 先尝试从缓存获取
    if let Some(cached) = get_from_cache(path) {
        tracing::info!("SoundfontCache: 命中缓存 {:?}", path);
        return Ok(cached);
    }

    // 缓存未命中，加载音色库
    tracing::info!("SoundfontCache: 缓存未命中，加载音色库 {:?}", path);
    let soundfont = SampleSoundfont::new(path, params, Default::default())
        .map_err(|e| format!("Failed to load soundfont: {:?}", e))?;

    let soundfont: Arc<dyn SoundfontBase> = Arc::new(soundfont);

    // 添加到缓存
    add_to_cache(path, soundfont.clone());

    Ok(soundfont)
}
