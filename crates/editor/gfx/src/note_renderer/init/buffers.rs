//! `NoteRenderer` 的缓冲 / 绑定组构造（从 `init.rs` 拆出，REF-001 < 400 行）。
//!
//! 这里全是 `new_with_depth` 的机械步骤，与管线状态无关；拆开只为让 `init.rs`
//! 保持可读长度，调用顺序与语义完全不变。

use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 构建间接绘制初始化模板（MAX_CHUNKS 槽位 × 默认 DrawIndirectArgs）。
    ///
    /// `prepare_pass` 每帧将其写回 indirect buffer 做重置；预计算后每帧零分配。
    pub(super) fn build_indirect_template(slot_align: u64) -> Vec<u8> {
        let slot_count = super::super::chunk::MAX_CHUNKS;
        let mut template = vec![0u8; (slot_count as u64 * slot_align) as usize];
        let default_args = super::super::types::DrawIndirectArgs::default();
        let default_bytes = bytemuck::bytes_of(&default_args);
        for idx in 0..slot_count {
            let offset = (idx as u64) * slot_align;
            template[offset as usize..offset as usize + default_bytes.len()]
                .copy_from_slice(default_bytes);
        }
        template
    }

    /// 创建计算 bind groups（分块切片）。
    ///
    /// binding 5 = 视图状态 uniform（轨道闸门用），与 render bind group 的
    /// binding 1 是同一个 buffer（见 `cull.wgsl::track_visible`）。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_cull_bind_groups(
        device: &wgpu::Device,
        chunk_layout: &super::super::chunk::ChunkLayout,
        gpu_note_buffer: &crate::gpu_note_buffer::GpuNoteBuffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        cull_uniform_buffer_size: &u64,
        viewport_buffer: &wgpu::Buffer,
        view_state_buffer: &wgpu::Buffer,
        cull_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Vec<wgpu::BindGroup> {
        Self::create_cull_bind_groups(
            device,
            chunk_layout,
            gpu_note_buffer.buffer(),
            visible_instance_buffer,
            indirect_buffer,
            cull_uniform_buffer,
            view_state_buffer,
            viewport_buffer,
            *cull_uniform_buffer_size,
            cull_bind_group_layout,
        )
    }
}
