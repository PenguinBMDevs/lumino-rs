//! 可见区间裁剪（cull range）分发。
//!
//! # 为什么需要它
//!
//! 全量 cull 每帧读取**全部** `NoteInstance`（16B/音符）：RTX 2060 实测
//! 64M 音符 cull 3.53ms、带宽 ≈220GB/s，而放大视图可见音符只有 6 万——
//! 99.9% 的读取是纯浪费。本模块按 UI 算出的可见 tick 区间，只对候选
//! 实例区间 dispatch cull：每个区间走一次 compute dispatch，用
//! `CullUniform.local_start` 把 workgroup 偏移到区间起点，indirect 计数仍
//! 按 chunk 累加，draw 路径完全不变。
//!
//! 一致性保障：
//! - 区间总数必须等于渲染器当前实例数（`CullPlan.total_instances`），
//!   否则回退整段 cull（流式上传/增量在途时 UI 与 GPU 可能差一帧）；
//! - 区间必须升序、互不重叠；越界自动裁剪；区间数超上限回退整段；
//! - 区间跨 chunk 时自动切分，每段绑定对应 chunk 的 bind group。

use super::NoteRenderer;
use super::chunk::MAX_CHUNKS;
use super::types::{CameraUniform, CullPlan, CullRange, CullUniform};

/// 一帧最多接受的区间数（超过则回退整段 cull，防 dispatch 风暴）。
pub(super) const MAX_CULL_RANGES: usize = 1024;

/// 与 cull.wgsl 的 `@workgroup_size(256)` 一致。
const WORKGROUP_SIZE: u32 = 256;
/// dispatch X 维上限。
const MAX_DISPATCH_X: u32 = 65535;

/// 切分后的单个 dispatch 区间（chunk 局部坐标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SplitRange {
    /// 全局起始实例索引
    pub(super) start: u32,
    /// 本段实例数
    pub(super) len: u32,
    /// 所属 chunk 索引
    pub(super) chunk_idx: usize,
    /// 在 chunk 内的局部起始偏移
    pub(super) local_start: u32,
}

/// 把全局候选区间按 chunk 边界切成可 dispatch 的段。
///
/// 前置：`ranges` 按 `start` 升序、互不重叠（UI 侧保证）。若发现乱序/重叠，
/// 返回空 Vec，调用方回退整段 cull（正确性优先）。
pub(super) fn split_ranges(
    ranges: &[CullRange],
    instance_count: usize,
    instances_per_chunk: usize,
) -> Vec<SplitRange> {
    if ranges.is_empty() || instance_count == 0 || instances_per_chunk == 0 {
        return Vec::new();
    }
    let count = u32::try_from(instance_count).unwrap_or(u32::MAX);
    let per_chunk = u32::try_from(instances_per_chunk).unwrap_or(u32::MAX);
    let mut out = Vec::new();
    let mut cursor = 0u32;

    for range in ranges {
        if range.count == 0 || range.start >= count {
            continue;
        }
        // 升序 + 不重叠契约：违反即回退（防可见索引重复写入）
        if range.start < cursor {
            return Vec::new();
        }
        let mut start = range.start;
        let end = range.start.saturating_add(range.count).min(count);
        while start < end {
            let chunk_idx = (start / per_chunk) as usize;
            if chunk_idx >= MAX_CHUNKS {
                return Vec::new();
            }
            let chunk_begin = chunk_idx as u32 * per_chunk;
            let chunk_end = chunk_begin.saturating_add(per_chunk);
            let seg_end = end.min(chunk_end);
            let local_start = start - chunk_begin;
            let len = seg_end - start;
            if len == 0 {
                break;
            }
            out.push(SplitRange {
                start,
                len,
                chunk_idx,
                local_start,
            });
            // 单 range 内也可能跨多个 chunk（track 段 > 2GB binding 上限）
            start = seg_end;
        }
        cursor = end;
    }
    out
}

impl NoteRenderer {
    /// 按候选区间执行 cull；计划不可用/不一致时回退整段 cull。
    pub fn prepare_pass_with_cull_plan(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        camera: CameraUniform,
        queue: &wgpu::Queue,
        plan: &CullPlan,
    ) {
        let count = self.last_upload_count as usize;
        if !plan.is_active() || plan.total_instances != count {
            self.prepare_pass(encoder, camera, queue);
            return;
        }
        let splits = split_ranges(&plan.ranges, count, self.chunk_layout.instances_per_chunk);
        // 区间数/覆盖范围异常 → 回退整段，绝不静默漏音
        if splits.is_empty()
            || splits.len() > MAX_CULL_RANGES
            || splits
                .iter()
                .any(|split| split.chunk_idx >= self.cull_bind_groups.len())
        {
            self.prepare_pass(encoder, camera, queue);
            return;
        }

        tracing::trace!(
            "NoteRenderer: range cull 生效 ranges={} splits={} total={}",
            plan.ranges.len(),
            splits.len(),
            count
        );

        // 与整段 cull 相同的帧前重置：indirect 计数清零 + 相机 uniform
        queue.write_buffer(self.indirect_buffer.inner(), 0, &self.indirect_template);
        queue.write_buffer(
            self.viewport_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );

        // 区间 uniform 从 chunk 槽位之后开始（chunk 槽位仍由 update_cull_info 写入）
        let slot_align = self.chunk_layout.slot_align as usize;
        let range_base = MAX_CHUNKS * slot_align;
        let mut bytes = vec![0u8; splits.len() * slot_align];
        for (idx, split) in splits.iter().enumerate() {
            let uniform = CullUniform {
                instance_count: u32::try_from(count).unwrap_or(u32::MAX),
                chunk_start: split.start - split.local_start,
                chunk_count: split.len,
                local_start: split.local_start,
            };
            let offset = idx * slot_align;
            bytes[offset..offset + std::mem::size_of::<CullUniform>()]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        queue.write_buffer(self.cull_uniform_buffer.inner(), range_base as u64, &bytes);

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("note_cull_range_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.cull_pipeline);
        for (idx, split) in splits.iter().enumerate() {
            let offset = (range_base + idx * slot_align) as u32;
            compute_pass.set_bind_group(0, &self.cull_bind_groups[split.chunk_idx], &[offset]);
            let workgroups = split.len.div_ceil(WORKGROUP_SIZE);
            let dispatch_x = workgroups.min(MAX_DISPATCH_X);
            let dispatch_y = workgroups.div_ceil(MAX_DISPATCH_X);
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u32, count: u32) -> CullRange {
        CullRange { start, count }
    }

    #[test]
    fn test_split_inside_single_chunk() {
        let out = split_ranges(&[range(100, 50)], 1000, 1000);
        assert_eq!(
            out,
            vec![SplitRange {
                start: 100,
                len: 50,
                chunk_idx: 0,
                local_start: 100
            }]
        );
    }

    #[test]
    fn test_split_across_chunk_boundary() {
        let out = split_ranges(&[range(90, 30)], 1000, 100);
        assert_eq!(out.len(), 2, "跨 chunk 的区间必须切成两段");
        assert_eq!(out[0].chunk_idx, 0);
        assert_eq!(out[0].local_start, 90);
        assert_eq!(out[0].len, 10);
        assert_eq!(out[1].chunk_idx, 1);
        assert_eq!(out[1].local_start, 0);
        assert_eq!(out[1].len, 20);
    }

    #[test]
    fn test_split_clamps_to_instance_count() {
        let out = split_ranges(&[range(990, 100)], 1000, 1000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len, 10, "越界部分必须裁掉");
    }

    #[test]
    fn test_split_rejects_overlap_and_unsorted() {
        assert!(
            split_ranges(&[range(10, 20), range(20, 5)], 1000, 1000).is_empty(),
            "区间重叠必须回退整段 cull"
        );
        assert!(
            split_ranges(&[range(30, 5), range(10, 5)], 1000, 1000).is_empty(),
            "乱序区间必须回退整段 cull"
        );
    }

    #[test]
    fn test_split_empty_range_or_zero_count() {
        assert!(split_ranges(&[], 1000, 100).is_empty());
        assert!(split_ranges(&[range(0, 0)], 1000, 100).is_empty());
        assert!(split_ranges(&[range(100, 10)], 0, 100).is_empty());
        assert!(split_ranges(&[range(100, 10)], 1000, 0).is_empty());
    }

    #[test]
    fn test_split_multiple_ranges_offsets() {
        let out = split_ranges(&[range(0, 10), range(500, 10), range(1200, 10)], 2000, 1000);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].chunk_idx, 0);
        assert_eq!(out[1].chunk_idx, 0);
        assert_eq!(out[2].chunk_idx, 1);
        assert_eq!(out[2].local_start, 200);
    }
}
