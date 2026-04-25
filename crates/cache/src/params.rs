//! 可调优参数与默认值
//!
//! 所有缓存相关的魔术数字集中在此处，便于调优。
//! 详见 crate 根目录的 `tuning.md` 了解调整策略。

/// 每个事件块的 tick 跨度（65536 ticks ≈ 典型 MIDI 1-2 秒）
///
/// 根据黑乐谱测试（如 164M 音符的 .mid 文件），65536 的块大小保证
/// 2000-5000 个块（取决于文件 tick 总数），索引可常驻内存。
///
/// 调大 → 块更少但更大，索引更小
/// 调小 → 块更多但更小，索引更大（但随机访问粒度更细）
pub const CHUNK_TICK_SPAN: u32 = 65536;

/// Windows PageBackend 的页大小（64 KB）
///
/// 64 KB 是 Windows 用户态内存分配的最小推荐粒度。
/// VirtualAlloc 实际分配粒度为 64 KB。
pub const WINDOWS_PAGE_SIZE: usize = 65536;

/// 默认 LRU 最大页数（Windows 最低保障）
///
/// 64 KB/page × 512 = 32 MB 基础缓存
pub const DEFAULT_MAX_PAGES: usize = 512;

/// Windows 系统内存百分比（用于动态计算 max_pages）
///
/// 默认取系统可用物理内存的 10%
pub const SYSTEM_MEMORY_PERCENT: f64 = 0.10;

/// L2 ChunkCache 最大条目数（按数量，用于小 chunk 场景的兜底）
pub const L2_MAX_CHUNKS: usize = 128;

/// L2 ChunkCache 内存预算（字节）。超过此值将淘汰最旧 chunk。
/// 默认 500 MB = 黑乐谱典型最大 chunk 大小(~440MB) + 余量
pub const L2_MEMORY_BUDGET: usize = 500_000_000;

/// L1 HotCache 时间窗口（当前播放位置前后秒数）
///
/// 秒数是根据典型 PPQN=480, tempo=120BPM 估算的粗略值。
/// 实际 tick 数 = 秒数 × PPQN × (BPM/60)
pub const L1_WINDOW_SECONDS: f64 = 2.0;

/// L1 最大事件数（防止极端情况下 OOM）
pub const L1_MAX_EVENTS: usize = 100_000;

/// 预取线程前方前瞻块数
pub const PREFETCH_AHEAD_COUNT: usize = 4;

/// 预取线程轮询间隔（毫秒）
pub const PREFETCH_POLL_MS: u64 = 10;

/// 索引文件魔数（".black" 格式标识）
pub const INDEX_MAGIC: &[u8; 8] = b"LUMIBLK1";

/// .black 索引格式版本
pub const INDEX_FORMAT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_are_reasonable() {
        // CHUNK_TICK_SPAN must be > 0 and power-of-2 friendly
        assert!(CHUNK_TICK_SPAN > 0);
        assert!(CHUNK_TICK_SPAN.is_power_of_two());

        // Window sizes must be reasonable
        assert!(WINDOWS_PAGE_SIZE >= 4096);
        assert!(DEFAULT_MAX_PAGES >= 16);
        assert!(L2_MAX_CHUNKS >= 4);
        assert!(L1_MAX_EVENTS >= 1000);

        // Percentages must be valid
        assert!((0.0..=1.0).contains(&SYSTEM_MEMORY_PERCENT));
        assert!(L1_WINDOW_SECONDS > 0.0);
    }
}
