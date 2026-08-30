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

use crate::gpu_resource_tracker::TrackedBuffer;

pub use types::{ArrangementNoteInstance, ArrangementUniform, colors};

/// 走带视图渲染器
pub struct ArrangementRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// Uniform 缓冲区
    uniform_buffer: TrackedBuffer,
    /// 覆盖层实例缓冲区（背景/lane/网格/框选/指示线，每帧重建）
    overlay_buffer: TrackedBuffer,
    /// 覆盖层当前容量
    overlay_capacity: usize,
    /// 覆盖层当前实例数
    overlay_count: u32,
    /// 音符实例缓冲区（note-space，常驻 GPU，仅音符数据变化时重建）
    note_buffer: TrackedBuffer,
    /// 音符缓冲当前容量
    note_capacity: usize,
    /// 音符缓冲当前实例数
    note_count: u32,
    /// 覆盖层中"背景层"实例数（背景/lane/网格），绘制时排在音符之下
    overlay_back_len: u32,
    /// Bind group
    bind_group: wgpu::BindGroup,
}

/// 顶点着色器代码
const VERTEX_SHADER: &str = include_str!("shaders/arrangement.wgsl");

/// 初始实例缓冲区大小
const INITIAL_CAPACITY: usize = 4096;
