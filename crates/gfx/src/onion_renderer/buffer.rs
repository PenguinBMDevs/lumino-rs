use super::{OnionNote, OnionRenderer};

impl OnionRenderer {
    pub(crate) fn create_note_pool_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<OnionNote>()) as wgpu::BufferAddress;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_note_pool"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub(crate) fn create_instance_indices_buffer(
        device: &wgpu::Device,
        capacity: usize,
    ) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<u32>()) as wgpu::BufferAddress;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_instance_indices"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        })
    }

    pub(crate) fn create_compute_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        note_pool_buffer: &wgpu::Buffer,
        instance_indices_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_compute_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: note_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: instance_indices_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }

    pub(crate) fn create_render_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        instance_indices_buffer: &wgpu::Buffer,
        note_pool_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_render_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instance_indices_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: note_pool_buffer.as_entire_binding(),
                },
            ],
        })
    }

    pub(crate) fn rebuild_bind_groups(&mut self, device: &wgpu::Device) {
        self.compute_bind_group = Self::create_compute_bind_group(
            device,
            &self.compute_bind_group_layout,
            &self.viewport_buffer,
            &self.note_pool_buffer,
            &self.instance_indices_buffer,
            &self.indirect_buffer,
        );
        self.render_bind_group = Self::create_render_bind_group(
            device,
            &self.render_bind_group_layout,
            &self.viewport_buffer,
            &self.instance_indices_buffer,
            &self.note_pool_buffer,
        );
    }
}
