//! Miditrail Aura 光环渲染
//!
//! 负责在按下的琴键下方生成发光光环，并管理光环相关的纹理、管线、缓冲区和渲染调用。

use super::{AURA_TEXTURE_SIZE, MiditrailAuraInstanceGpu, MiditrailRenderer};

impl MiditrailRenderer {
    /// 确保 Aura 实例缓冲区足够大。
    pub(super) fn ensure_aura_instance_buffer(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.aura_instance_capacity {
            return;
        }
        let new_cap = count
            .next_power_of_two()
            .max(Self::INITIAL_INSTANCE_CAPACITY);
        let size = (new_cap * std::mem::size_of::<MiditrailAuraInstanceGpu>()) as u64;
        // 旧缓冲由 Option::take 触发 Drop 自动注销
        let buffer = crate::gpu_resource_tracker::TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("miditrail_aura_instance_buffer"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.aura_instance_buffer = Some(buffer);
        self.aura_instance_capacity = new_cap;
    }

    /// 确保 Aura 纹理和视图已创建。
    pub(super) fn ensure_aura_resources(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.aura_resources_ready {
            return;
        }

        let texture = super::pipeline::create_aura_texture(
            device,
            queue,
            AURA_TEXTURE_SIZE,
            &self.aura_image_data,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.aura_texture_view = Some(view);
        self.aura_texture = Some(texture);
        self.aura_resources_ready = true;

        // 纹理改变后 bind group 需要重建
        self.bind_group = None;
    }

    /// 在同一个 render pass 中绘制 Aura 实例。
    pub(super) fn draw_aura(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        aura_instances: &[MiditrailAuraInstanceGpu],
    ) {
        if aura_instances.is_empty() {
            return;
        }
        // 不变式：draw_aura 在 render() 中 rebuild_bind_group 之后调用
        let Some(bind_group) = self.bind_group.as_ref() else {
            debug_assert!(false, "bind_group 应已初始化（rebuild_bind_group 已执行）");
            return;
        };
        let Some(aura_instance_buf) = self.aura_instance_buffer.as_ref() else {
            debug_assert!(
                false,
                "aura_instance_buffer 应已初始化（render 前 ensure_aura_instance_buffer 已调用）"
            );
            return;
        };

        render_pass.set_pipeline(&self.aura_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.aura_vertex_buffer.inner().slice(..));
        render_pass.set_vertex_buffer(1, aura_instance_buf.inner().slice(..));
        render_pass.set_index_buffer(
            self.aura_index_buffer.inner().slice(..),
            wgpu::IndexFormat::Uint16,
        );
        render_pass.draw_indexed(0..6, 0, 0..aura_instances.len() as u32);
    }
}
