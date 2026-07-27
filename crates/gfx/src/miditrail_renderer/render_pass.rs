//! Miditrail 渲染通道执行
//!
//! 负责把音符/琴键实例和 Aura 实例绘制到同一个离屏 render pass 中。

use super::{MiditrailAuraInstanceGpu, MiditrailInstanceGpu, MiditrailRenderer};

impl MiditrailRenderer {
    /// 执行完整的 Miditrail 渲染通道。
    pub(super) fn execute_render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[MiditrailInstanceGpu],
        aura_instances: &[MiditrailAuraInstanceGpu],
    ) {
        let color_view = self
            .output_texture_view
            .as_ref()
            .expect("output_texture_view 应已初始化");
        let depth_view = self
            .depth_texture_view
            .as_ref()
            .expect("depth_texture_view 应已初始化");
        let bind_group = self.bind_group.as_ref().expect("bind_group 应已初始化");
        let instance_buf = self
            .instance_buffer
            .as_ref()
            .expect("instance_buffer 应已初始化");

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("miditrail_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 先绘制音符与琴键
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, instance_buf.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(
                0..Self::CUBE_INDICES.len() as u32,
                0,
                0..instances.len() as u32,
            );

            // 再绘制 Aura（附加混合，不写入深度），使其环绕在音符立方体前缘
            self.draw_aura(&mut render_pass, aura_instances);
        }
    }
}
