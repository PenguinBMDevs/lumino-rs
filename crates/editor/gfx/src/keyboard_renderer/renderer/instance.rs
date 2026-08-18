use super::super::types::KeyInstance;
use super::KeyboardRenderer;
use crate::gpu_resource_tracker::{self, TrackedBuffer};

impl KeyboardRenderer {
    /// 创建实例缓冲区
    pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> TrackedBuffer {
        gpu_resource_tracker::create_instance_buffer::<KeyInstance>(
            device,
            "keyboard_instance_buffer",
            capacity,
        )
    }

    /// 实例缓冲区布局
    pub(super) fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<KeyInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // size
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // is_black
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // key_index
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}
