//! Bind group 重建

use super::WaterfallRenderer;

impl WaterfallRenderer {
    /// 重建 bind group（当 buffers 或 texture 变化时）。
    pub(crate) fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        let note_buf = self.note_buffer.as_ref().expect("note_buffer 未初始化");
        let key_colors_buf = self
            .active_key_colors_buffer
            .as_ref()
            .expect("active_key_colors_buffer 未初始化");
        let out_view = self
            .output_texture_view
            .as_ref()
            .expect("output_texture_view 未初始化");

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waterfall_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: note_buf.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: key_colors_buf.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self
                        .key_offsets_buffer
                        .as_ref()
                        .expect("key_offsets_buffer 未初始化")
                        .inner()
                        .as_entire_binding(),
                },
            ],
        });
        self.bind_group = Some(bind_group);
    }
}
