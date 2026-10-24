//! 钢琴瀑布流 source/visible buffer 的 binding 分段规划。
//!
//! 12B 音符 source 单 buffer 可大于 `max_storage_buffer_binding_size`，但每个
//! storage binding 的 offset/size 必须受限；按 chunk 切 bind，并在 draw 时逐
//! chunk 绑定 source/visible，索引保持 chunk 局部。

/// 一个 chunk 的全局实例范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyboardChunk {
    pub start: usize,
    pub count: usize,
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}

fn lcm(a: u64, b: u64) -> u64 {
    a / gcd(a, b) * b
}

/// 规划 chunk：保证 `start * 12` 与 `start * 4` 都满足 binding offset 对齐。
pub(super) fn plan_chunks(
    instance_count: usize,
    binding_limit: u64,
    offset_align: u64,
) -> Vec<KeyboardChunk> {
    if instance_count == 0 || binding_limit == 0 {
        return Vec::new();
    }
    // 12B source 与 4B visible 两种 offset 同时对齐。
    let step = lcm(
        offset_align / gcd(offset_align, 12),
        offset_align / gcd(offset_align, 4),
    )
    .max(1) as usize;
    let max_by_binding = (binding_limit / 12) as usize;
    let per_chunk = (max_by_binding / step * step).max(step);
    let mut out = Vec::with_capacity(instance_count.div_ceil(per_chunk));
    let mut start = 0usize;
    while start < instance_count {
        let count = (instance_count - start).min(per_chunk);
        out.push(KeyboardChunk { start, count });
        start += count;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_splits_at_binding_limit_with_alignment() {
        // 模拟 2GB-1 binding、256B offset 对齐：每 chunk 178,956,928 实例
        let chunks = plan_chunks(310_979_003, 2_147_483_647, 256);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].count, 178_956_928);
        assert_eq!(chunks[1].start, 178_956_928);
        assert_eq!(chunks[1].count, 132_022_075);
        for c in &chunks {
            assert_eq!((c.start as u64 * 12) % 256, 0);
            assert_eq!((c.start as u64 * 4) % 256, 0);
            assert!(c.count as u64 * 12 <= 2_147_483_647);
        }
    }

    #[test]
    fn test_plan_empty_or_single() {
        assert!(plan_chunks(0, 2_147_483_647, 256).is_empty());
        assert_eq!(plan_chunks(1, 2_147_483_647, 256).len(), 1);
    }
}
