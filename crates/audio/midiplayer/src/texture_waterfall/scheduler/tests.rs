//! 贴图瀑布流调度器单元测试

mod benchmark;
mod generation;

use crate::texture_waterfall::compute_waterfall_cache_hash;
use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::note::WaterfallNote;

fn make_note(start: u32, end: u32, key: u8, color: [u8; 4]) -> WaterfallNote {
    WaterfallNote::from_ms(start as f32, end as f32, key, color)
}

fn test_config() -> (TextureWaterfallConfig, String) {
    let mut config = TextureWaterfallConfig::default();
    let dir = std::env::temp_dir()
        .join("lumino-TextureWaterfall-sched-test")
        .join(format!("{}-{}", std::process::id(), unique_id()));
    let _ = std::fs::remove_dir_all(&dir);
    config.cache_dir = dir.clone();
    (config, compute_waterfall_cache_hash(b"sched-test"))
}

fn unique_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn cleanup(config: &TextureWaterfallConfig) {
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
