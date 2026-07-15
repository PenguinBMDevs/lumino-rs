//! 工程走带渲染器
//!
//! 使用 WGPU 实例化渲染 (Instance Rendering) 高效渲染工程走带视图，
//! 参考 yinhe 实现，包含音轨 lane 背景、小节网格线、音符实例和演奏指示线。
//!
//! 渲染方式：Vertex Buffer + Instance Rendering (TriangleList)
//! 每个实例包含 xywh (屏幕空间坐标) + packed 颜色/属性数据

mod draw;
mod init;
mod prepare;
mod types;

use crate::gpu_resource_tracker;

pub use types::{ArrangementNoteInstance, ArrangementUniform, colors};

/// 走带视图渲染器
pub struct ArrangementRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// Uniform 缓冲区
    uniform_buffer: wgpu::Buffer,
    /// 音符实例缓冲区（作为 vertex buffer 使用）
    instance_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// 当前容量
    capacity: usize,
    /// 上次上传的实例数
    last_instance_count: u32,
}

/// 顶点着色器代码
const VERTEX_SHADER: &str = include_str!("shaders/arrangement.wgsl");

/// 初始实例缓冲区大小
const INITIAL_CAPACITY: usize = 4096;

impl Drop for ArrangementRenderer {
    fn drop(&mut self) {
        gpu_resource_tracker::sub_buffer(&self.uniform_buffer);
        gpu_resource_tracker::sub_buffer(&self.instance_buffer);
    }
}
