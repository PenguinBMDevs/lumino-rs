//! Cull 绑定分块（chunking）布局计算
//!
//! wgpu 硬限制：单个 storage buffer binding 的 range 不得超过
//! `max_storage_buffer_binding_size`（常见设备 = 2GB-1 = 2147483647 字节）。
//! 2.9 亿音符 × 16B ≈ 4.6GB 的单 buffer 无法整体绑定，必须按 offset 切片：
//! 每个 chunk 一个 bind group + 一次 compute dispatch + 一次 draw_indirect。
//!
//! 本模块只做纯计算（可单测），不接触 GPU 资源。

/// 固定 chunk 槽位上限。
///
/// 每槽位可承载 `instances_per_chunk` 个实例（≈2GB/16B ≈ 1.34 亿），
/// 64 槽位 ≈ 86 亿实例（>137GB），远超任何设备的 max_buffer_size 实际容量。
pub(super) const MAX_CHUNKS: usize = 64;

/// 分块布局参数（从设备 limits 动态计算，跨硬件自适应）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ChunkLayout {
    /// 每个 chunk 的实例数（字节范围 256B 对齐且 ≤ max_storage_buffer_binding_size）
    pub instances_per_chunk: usize,
    /// binding offset 对齐字节数（storage/uniform 共同取 max）
    pub slot_align: u64,
}

impl ChunkLayout {
    /// 从设备 limits 计算分块布局
    pub(super) fn from_limits(limits: &wgpu::Limits) -> Self {
        let storage_align = limits.min_storage_buffer_offset_alignment;
        let uniform_align = limits.min_uniform_buffer_offset_alignment;
        let slot_align = storage_align.max(uniform_align).max(256) as u64;

        // binding range 上限向下取整到对齐值，再除以实例大小
        let max_bytes = limits.max_storage_buffer_binding_size as u64 / slot_align * slot_align;
        let instances_per_chunk =
            (max_bytes / std::mem::size_of::<crate::NoteInstance>() as u64).max(1) as usize;
        Self {
            instances_per_chunk,
            slot_align,
        }
    }

    /// 数据量（实例数）对应的 chunk 数
    pub(super) fn chunk_count(&self, instance_count: usize) -> usize {
        instance_count.div_ceil(self.instances_per_chunk)
    }

    /// 第 `idx` 个 chunk 的实例范围 `(start, len)`。
    /// `instance_count` 为数据量（非 buffer 容量）；`idx` 越界返回 `(0, 0)`。
    pub(super) fn chunk_range(&self, instance_count: usize, idx: usize) -> (usize, usize) {
        let start = idx.saturating_mul(self.instances_per_chunk);
        if start >= instance_count {
            return (0, 0);
        }
        let len = (instance_count - start).min(self.instances_per_chunk);
        (start, len)
    }

    /// 第 `idx` 个 chunk 的绑定 offset（字节），对齐到 slot_align
    pub(super) fn chunk_offset_bytes(&self, idx: usize) -> u64 {
        (idx as u64) * self.slot_align
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_renderer::types::NoteInstance;

    /// 默认 limits（模拟常见设备：2GB-1 binding 上限，256B 对齐）
    fn default_limits() -> wgpu::Limits {
        wgpu::Limits {
            max_storage_buffer_binding_size: 2_147_483_647, // 2GB - 1
            max_buffer_size: 4_294_967_296,
            min_storage_buffer_offset_alignment: 256,
            min_uniform_buffer_offset_alignment: 256,
            ..wgpu::Limits::default()
        }
    }

    #[test]
    fn test_layout_2gb_binding_cap() {
        let layout = ChunkLayout::from_limits(&default_limits());
        // 2GB-1 向下对齐到 256 → 2_147_483_392 字节 / 16B = 134_217_712 实例
        assert_eq!(layout.instances_per_chunk, 134_217_712);
        assert_eq!(layout.slot_align, 256);
        // 每 chunk 字节范围 ≤ 2GB-1
        assert!(
            (layout.instances_per_chunk as u64) * (std::mem::size_of::<NoteInstance>() as u64)
                <= 2_147_483_647
        );
        // chunk 起始 offset 满足 256B 对齐
        assert_eq!(layout.chunk_offset_bytes(1), 256);
    }

    #[test]
    fn test_chunk_count() {
        let layout = ChunkLayout::from_limits(&default_limits());
        assert_eq!(layout.chunk_count(0), 0);
        assert_eq!(layout.chunk_count(1), 1);
        assert_eq!(layout.chunk_count(layout.instances_per_chunk), 1);
        assert_eq!(layout.chunk_count(layout.instances_per_chunk + 1), 2);
        // 2.9 亿音符 → 3 chunks
        assert_eq!(layout.chunk_count(290_000_000), 3);
    }

    #[test]
    fn test_chunk_range() {
        let layout = ChunkLayout::from_limits(&default_limits());
        let per = layout.instances_per_chunk;

        // 单 chunk
        assert_eq!(layout.chunk_range(10, 0), (0, 10));
        assert_eq!(layout.chunk_range(10, 1), (0, 0));

        // 恰好整除
        assert_eq!(layout.chunk_range(per * 2, 0), (0, per));
        assert_eq!(layout.chunk_range(per * 2, 1), (per, per));
        assert_eq!(layout.chunk_range(per * 2, 2), (0, 0));

        // 非整除：最后一块是余数
        let total = per * 2 + 123;
        assert_eq!(layout.chunk_range(total, 2), (per * 2, 123));

        // 范围无重叠、无空洞
        let total = 290_000_000;
        let mut prev_end = 0;
        for idx in 0..layout.chunk_count(total) {
            let (start, len) = layout.chunk_range(total, idx);
            assert_eq!(start, prev_end);
            assert!(len > 0 && len <= per);
            prev_end = start + len;
        }
        assert_eq!(prev_end, total);
    }

    #[test]
    fn test_small_binding_limit_never_zero() {
        // 极端小 binding 上限（防御：instances_per_chunk 至少 1）
        let mut limits = default_limits();
        limits.max_storage_buffer_binding_size = 256;
        let layout = ChunkLayout::from_limits(&limits);
        assert!(layout.instances_per_chunk >= 1);
    }

    #[test]
    fn test_slot_align_uses_max_of_limits() {
        let mut limits = default_limits();
        limits.min_uniform_buffer_offset_alignment = 512;
        let layout = ChunkLayout::from_limits(&limits);
        assert_eq!(layout.slot_align, 512);
        // offset 对齐后 binding 字节范围仍不超上限
        assert!(
            (layout.instances_per_chunk as u64) * 16
                <= limits.max_storage_buffer_binding_size as u64
        );
    }
}
