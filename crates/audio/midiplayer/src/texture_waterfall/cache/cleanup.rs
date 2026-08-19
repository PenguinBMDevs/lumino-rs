//! 缓存清理与淘汰逻辑
//!
//! 提供按 MIDI 哈希清理和全量清理两种策略。

use std::path::Path;

use super::core::WaterfallCacheError;

/// 清理指定 MIDI 的所有缓存文件，返回删除数量
pub fn clear_midi_waterfall_cache(
    cache_dir: &Path,
    midi_hash: &str,
) -> Result<u32, WaterfallCacheError> {
    let prefix = format!("{midi_hash}_");
    let mut count = 0;
    if cache_dir.exists() {
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && name.starts_with(&prefix)
                && name.ends_with(".lmocache")
            {
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// 清理缓存目录下全部 `.lmocache` 文件，返回删除数量
pub fn clear_all_waterfall_cache(cache_dir: &Path) -> Result<u32, WaterfallCacheError> {
    let mut count = 0;
    if cache_dir.exists() {
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".lmocache")
            {
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
    }
    Ok(count)
}
