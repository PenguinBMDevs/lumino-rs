use super::super::MiditrailRenderer;
use crate::NoteInstance;
use crate::gpu_resource_tracker::TrackedBuffer;

impl MiditrailRenderer {
    /// 首帧全量播种：常驻缓冲一次上传 + cull 世代递增（桶下次提取时构建）。    ///
    /// 调用方（导出 handler）在 `params.note_instances` 非空时调用；后续空帧跳过。
    pub fn seed_resident(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        notes: &[NoteInstance],
    ) {
        let need = notes.len().max(1);
        let cap = self.resident_capacity;
        if self.resident_buffer.is_none() || need > cap {
            let new_cap = (need.saturating_mul(6) / 5).max(need + 1024);
            let size = (new_cap * 16) as u64;
            self.resident_buffer = Some(TrackedBuffer::new(
                device,
                &wgpu::BufferDescriptor {
                    label: Some("miditrail_export_resident"),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ));
            self.resident_capacity = new_cap;
        }
        if let Some(ref buf) = self.resident_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(notes));
        }
        self.resident_count = notes.len();
        self.resident_cull.mark_resident_updated();
    }

    /// 释放导出全套 GPU/CPU 常驻（完成/取消后由 `FinishVideoExport` 调用）。
    ///
    /// 释放：全量常驻缓冲（`seed_resident` 一次上传，24M 文档约 370MB）、
    /// cull 提取器全套（桶/sort_index/compact，约 300MB）、回读暂存、
    /// 实例缓冲 + Aura 实例缓冲（历史峰值，`next_power_of_two` 只增不减）、
    /// 导出专用 CPU 切片（`cull_cpu`/`scratch_derived`）。
    /// 保留：管线/布局/纹理/采样器（编译产物或小常量，下次导出复用）与
    /// 通用构建暂存（`scratch_notes/keys/auras`，`clear+reserve` 语义，
    /// 下次导出首帧即复用，无需重分配）。
    /// 下次 `seed_resident` 按需重建 + 世代递增 → 桶重建，冷启动与首启一致。
    pub fn release_export_resources(&mut self) {
        self.resident_buffer = None;
        self.resident_capacity = 0;
        self.resident_count = 0;
        self.resident_cull.release();
        self.cull_staging = None;
        self.cull_cpu = Vec::new();
        self.scratch_derived = Vec::new();
        self.instance_buffer = None;
        self.instance_capacity = 0;
        self.aura_instance_buffer = None;
        self.aura_instance_capacity = 0;
    }
}
