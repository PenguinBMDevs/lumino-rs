use super::{ArrangementNoteInstance, ArrangementRenderer, ArrangementUniform};
use std::time::Instant;

impl ArrangementRenderer {
    /// 准备渲染数据
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: ArrangementUniform,
        instances: &[ArrangementNoteInstance],
    ) {
        puffin::profile_scope!("arrangement::gpu_upload");
        let t0 = Instant::now();

        // 更新 uniform
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[uniform]),
        );

        let instance_count = instances.len();

        // 更新 instance buffer（作为 vertex buffer）
        if instance_count > 0 {
            let cap_t0 = Instant::now();
            Self::ensure_capacity(
                &mut self.instance_buffer,
                &mut self.capacity,
                device,
                instance_count,
            );
            let grow_ms = cap_t0.elapsed().as_secs_f64() * 1000.0;
            queue.write_buffer(
                self.instance_buffer.inner(),
                0,
                bytemuck::cast_slice(instances),
            );
            let upload_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let bytes = instance_count * std::mem::size_of::<ArrangementNoteInstance>();
            tracing::debug!(
                target: "perf::arrangement",
                instances = instance_count,
                bytes,
                grow_ms,
                upload_ms,
                "gpu_upload"
            );
        }

        self.last_instance_count = instance_count as u32;
    }
}
