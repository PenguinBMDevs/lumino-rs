use super::types::{CameraUniform, CullUniform, DrawIndirectArgs};
use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 兼容方法：数据+camera一步准备好（内部仍拆分成两步）
    pub fn prepare_old(
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

    /// 仅在音符数据真正变化时调用：负责更新 uniform
    pub fn update_cull_info(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        puffin::profile_function!();
        let current_count = self.gpu_note_buffer.len();
        self.last_upload_count = current_count as u32;

        if current_count == 0 {
            return;
        }

        // 上传 cull uniform
        let cull_info = CullUniform {
            instance_count: self.last_upload_count,
            _padding: [0; 3],
        };
        queue.write_buffer(
            &self.cull_uniform_buffer,
            0,
            bytemuck::cast_slice(&[cull_info]),
        );

        // 如果 GpuNoteBuffer 扩容了，或者 visible_instance_buffer 也需要扩容
        let required_capacity = current_count;
        if required_capacity > self.capacity {
            self.grow_visible_buffer(device, required_capacity);
        }

        // 重新绑定（如果 gpu_note_buffer 的内存变了，必须重新绑定）
        self.cull_bind_group = Self::create_cull_bind_group(
            device,
            &self.cull_bind_group_layout,
            &self.viewport_buffer,
            &self.cull_uniform_buffer,
            self.gpu_note_buffer.buffer(),
            &self.visible_instance_buffer,
            &self.indirect_buffer,
            current_count,
        );
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
