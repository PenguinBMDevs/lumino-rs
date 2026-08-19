use super::{ArrangementRenderer, ArrangementUniform};

impl ArrangementRenderer {
    /// 准备渲染数据
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: ArrangementUniform,
        instances: &[super::ArrangementNoteInstance],
    ) {
        // 更新 uniform
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[uniform]),
        );

        let instance_count = instances.len();

        // 更新 instance buffer（作为 vertex buffer）
        if instance_count > 0 {
            Self::ensure_capacity(
                &mut self.instance_buffer,
                &mut self.capacity,
                device,
                instance_count,
            );
            queue.write_buffer(
                self.instance_buffer.inner(),
                0,
                bytemuck::cast_slice(instances),
            );
        }

        self.last_instance_count = instance_count as u32;
    }
}
