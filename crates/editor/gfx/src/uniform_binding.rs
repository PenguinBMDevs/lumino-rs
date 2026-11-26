//! 单值 uniform 的 buffer + bind group + 脏检查。
//!
//! 三个渲染器（grid / cc_bar / ruler）此前各自重复同一段
//! 「BindGroupLayout → TrackedBuffer → BindGroup → 变化才 write」样板。
//! 这里收成一份；新渲染器只需给出 `T` 与 visibility。

use bytemuck::Pod;

use crate::gpu_resource_tracker::TrackedBuffer;

/// 一个单值 uniform 的完整绑定（binding 0、无 dynamic offset）。
pub(crate) struct UniformBinding<T: Pod> {
    buffer: TrackedBuffer,
    bind_group: wgpu::BindGroup,
    cached: Option<T>,
}

impl<T: Pod + PartialEq + Copy> UniformBinding<T> {
    /// 创建只含一个 uniform entry 的 BindGroupLayout。
    pub(crate) fn create_layout(
        device: &wgpu::Device,
        label: &str,
        visibility: wgpu::ShaderStages,
    ) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    /// 创建 uniform buffer 与 bind group。
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        initial: T,
        buffer_label: &str,
        bind_group_label: &str,
    ) -> Self {
        let buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some(buffer_label),
                contents: bytemuck::cast_slice(&[initial]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(bind_group_label),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.inner().as_entire_binding(),
            }],
        });
        Self {
            buffer,
            bind_group,
            cached: None,
        }
    }

    /// 值变化时才上传；返回是否真的写了 buffer。
    ///
    /// 首帧 `cached` 为 `None`，与初始值相同也会写一次——与原有实现一致
    /// （初始化数据与 `initial` 相同，多写一次无副作用）。
    pub(crate) fn write(&mut self, queue: &wgpu::Queue, value: T) -> bool {
        if self.cached.as_ref() == Some(&value) {
            return false;
        }
        queue.write_buffer(self.buffer.inner(), 0, bytemuck::cast_slice(&[value]));
        self.cached = Some(value);
        true
    }

    /// 绑定用 bind group。
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
