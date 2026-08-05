use super::buffer::CullBuffers;
use super::types::{CameraUniform, CullUniform, DrawIndirectArgs};
use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 从外部 slice 上传音符实例到 GPU（不含 compute pass）
    ///
    /// 用于分离渲染线程从双缓冲读取数据后直接上传，
    /// 相比 `prepare_notes` 不包含 compute cull（由后续的 `prepare_pass` 完成）
    pub fn upload_instances(
        &mut self,
        instances: &[crate::NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.gpu_note_buffer.upload_all(instances);
        self.update_cull_info(device, queue);
    }

    /// 流式上传：开始一次流式上传会话（用于 6 亿音符场景控制 CPU 峰值）
    ///
    /// 调用方应在 UI 线程分块构建 NoteInstance，逐块调用 `streaming_append`，
    /// 最后调用 `finish_streaming_upload`。避免 `upload_instances` 的 CPU 全量副本。
    ///
    /// 详见 `GpuNoteBuffer::begin_streaming_upload`。
    pub fn begin_streaming_upload(&mut self) {
        self.gpu_note_buffer.begin_streaming_upload();
    }

    /// 流式上传：追加一块音符实例到 GPU buffer
    ///
    /// 每块大小建议 ≤ 10 万实例（1.6 MB）控制单次 write_buffer 内存峰值。
    pub fn streaming_append(&mut self, chunk: &[crate::NoteInstance]) {
        self.gpu_note_buffer.streaming_append(chunk);
    }

    /// 流式上传：结束会话并更新 cull info
    ///
    /// 必须在所有 `streaming_append` 完成后调用，以更新 bind group / uniform。
    pub fn finish_streaming_upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.gpu_note_buffer.finish_streaming_upload();
        self.update_cull_info(device, queue);
    }

    // ── 洋葱皮事件级增量专用（流式模式安全，无 CPU 缓存） ────────────────────
    //
    // 薄封装 GpuNoteBuffer 的流式安全操作，供渲染线程的 OnionSegment 段表增量使用。
    // 流式模式下 update_note/update_notes 会索引已清空的 CPU 缓存而 panic，
    // 因此段替换必须走以下纯 GPU 方法。

    /// 当前 GPU 实例数（段表偏移计算用）
    pub fn gpu_instance_count(&self) -> usize {
        self.gpu_note_buffer.len()
    }

    /// 当前 GPU 容量（实例数）
    pub fn gpu_capacity(&self) -> usize {
        self.gpu_note_buffer.capacity()
    }

    /// 扩容（grow 会重建 buffer 并 GPU 内部复制现有数据）
    pub fn grow_gpu(&mut self, required: usize) -> bool {
        self.gpu_note_buffer.grow(required)
    }

    /// 段内写（不更新计数，不触碰 CPU 缓存）
    pub fn write_segment(&mut self, offset: usize, instances: &[crate::NoteInstance]) {
        self.gpu_note_buffer.write_segment(offset, instances);
    }

    /// 设置实例计数（变长段替换后更新）
    pub fn set_gpu_instance_count(&mut self, count: usize) {
        self.gpu_note_buffer.set_instance_count(count);
    }

    /// GPU 内部搬移（staging 分块，支持重叠）
    pub fn move_gpu_range(&mut self, src: usize, dst: usize, count: usize) {
        self.gpu_note_buffer.move_range(src, dst, count);
    }

    /// 上传音符实例并准备渲染（推荐的替代方案，替代 `prepare_old`）
    pub fn prepare_notes(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[crate::NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
    ) {
        puffin::profile_function!();
        self.gpu_note_buffer.upload_all(instances);
        self.update_cull_info(device, queue);
        self.prepare_pass(encoder, camera, queue);
    }

    /// 兼容方法：数据+camera一步准备好（内部仍拆分成两步）
    /// 请改用 [`prepare_notes`]。
    #[deprecated(since = "0.1.0", note = "请改用 prepare_notes()")]
    pub fn prepare_old(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[crate::NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
    ) {
        self.prepare_notes(encoder, instances, device, queue, camera);
    }

    /// 仅在音符数据真正变化时调用：负责更新 uniform
    ///
    /// 优化：bind group 在 count 不变且未扩容时 SKIP 创建。
    /// 因为 data 写入 GPU 后通过同一 buffer 句柄可见，无需新 bind group。
    /// 火焰图显示 create_cull_bind_group 是每帧瓶颈之一，此优化消除它。
    pub fn update_cull_info(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        puffin::profile_function!();
        let current_count = self.gpu_note_buffer.len();

        if current_count == 0 {
            self.last_upload_count = 0;
            return;
        }

        // 上传 cull uniform（write_buffer 开销极小，每次更新）
        let cull_info = CullUniform {
            instance_count: current_count as u32,
            _padding: [0; 3],
        };
        queue.write_buffer(
            &self.cull_uniform_buffer,
            0,
            bytemuck::cast_slice(&[cull_info]),
        );

        // 扩容 + 按需重建 bind group
        let required_capacity = current_count;
        let did_grow = required_capacity > self.capacity;
        if did_grow {
            self.grow_visible_buffer(device, required_capacity);
        }

        let count_changed = current_count != self.last_upload_count as usize;
        if did_grow || count_changed {
            // 实例数变化或扩容 → 绑定的 buffer size 变了 → 必须重建 bind group
            let cull_buffers = CullBuffers {
                layout: &self.cull_bind_group_layout,
                viewport_buffer: &self.viewport_buffer,
                cull_uniform_buffer: &self.cull_uniform_buffer,
                instance_buffer: self.gpu_note_buffer.buffer(),
                visible_instance_buffer: &self.visible_instance_buffer,
                indirect_buffer: &self.indirect_buffer,
                instance_count: current_count,
            };
            self.cull_bind_group = Self::create_cull_bind_group(device, &cull_buffers);
        }
        // else: 实例数相同且未扩容 → 旧 bind group 仍有效，跳过重建

        self.last_upload_count = current_count as u32;
    }

    /// 滚动/缩放等视口变化时调用：只更新 camera 并重跑 compute cull
    pub fn prepare_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        camera: CameraUniform,
        queue: &wgpu::Queue,
    ) {
        puffin::profile_function!();
        if self.last_upload_count == 0 {
            // 仍有 0 个实例要绘制，但必须重置间接缓冲，
            // 否则 draw_indirect 会使用上一帧的旧绘制参数（旧 instance_count）
            // 导致 GPU 从实例缓冲中读取陈旧的音符数据并渲染出幽灵音符。
            let indirect_args = DrawIndirectArgs {
                vertex_count: 4,
                instance_count: 0,
                first_vertex: 0,
                first_instance: 0,
                _padding: [0; 4],
            };
            queue.write_buffer(
                &self.indirect_buffer,
                0,
                bytemuck::cast_slice(&[indirect_args]),
            );
            return;
        }
        // 上传 viewport uniform
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[camera]));

        // 重置间接绘制参数 (instance_count = 0)
        let indirect_args = DrawIndirectArgs {
            vertex_count: 4,
            instance_count: 0,
            first_vertex: 0,
            first_instance: 0,
            _padding: [0; 4],
        };
        queue.write_buffer(
            &self.indirect_buffer,
            0,
            bytemuck::cast_slice(&[indirect_args]),
        );

        // 执行 Compute Culling
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("note_cull_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.cull_pipeline);
        compute_pass.set_bind_group(0, &self.cull_bind_group, &[]);

        // 计算工作组数量 (每组 256 个线程，与 shader 中的 workgroup_size 匹配)
        // 使用 256 可以更好地利用 modern GPU 的 warp/wavefront 大小
        const WORKGROUP_SIZE: u32 = 256;
        const MAX_DISPATCH_X: u32 = 65535;
        let workgroup_count = self.last_upload_count.div_ceil(WORKGROUP_SIZE);
        let dispatch_x = workgroup_count.min(MAX_DISPATCH_X);
        let dispatch_y = workgroup_count.div_ceil(MAX_DISPATCH_X);
        compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
    }
}
