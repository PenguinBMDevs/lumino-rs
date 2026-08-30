//! 工程走带渲染器
//!
//! 使用 WGPU 实例化渲染 (Instance Rendering) 高效渲染工程走带视图，
//! 参考 yinhe 实现，包含音轨 lane 背景、小节网格线、音符实例和演奏指示线。
//!
//! 渲染方式：Vertex Buffer + Instance Rendering (TriangleList)
//! 每个实例包含 xywh (屏幕空间坐标) + packed 颜色/属性数据
//!
//! 音符层（走带分音轨）：直接复用钢琴卷帘常驻 GPU 音符缓冲
//! （`onion_skin.gpu_note_buffer`），不再维护第二份音符缓冲 / 每帧重建，
//! 仅按可见音轨分段 draw，泳道映射通过 `lane_index` 存储缓冲完成。

mod draw;
mod init;
mod prepare;
mod types;

use crate::gpu_resource_tracker::TrackedBuffer;

pub use types::{
    ArrangementNoteInstance, ArrangementNoteUniform, ArrangementUniform, colors,
};

/// 走带视图渲染器
pub struct ArrangementRenderer {
    /// 覆盖层渲染管线（背景/lane/网格/框选/指示线，每帧重建的屏幕空间实例）
    pipeline: wgpu::RenderPipeline,
    /// 覆盖层 Uniform 缓冲区
    uniform_buffer: TrackedBuffer,
    /// 覆盖层实例缓冲区
    overlay_buffer: TrackedBuffer,
    /// 覆盖层当前容量
    overlay_capacity: usize,
    /// 覆盖层当前实例数
    overlay_count: u32,
    /// 覆盖层中"背景层"实例数（背景/lane/网格），绘制时排在音符之下
    overlay_back_len: u32,
    /// 覆盖层 Bind group
    bind_group: wgpu::BindGroup,

    /// 音符渲染管线 —— 复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存）
    note_pipeline: wgpu::RenderPipeline,
    /// 音符着色器 Uniform（滚动/缩放/泳道高/画布偏移）
    note_uniform_buffer: TrackedBuffer,
    /// 文档音轨 → 走带泳道序号 映射（存储缓冲，按 doc track 索引）
    lane_index_buffer: TrackedBuffer,
    /// lane_index 容量（f32 元素数）
    lane_index_capacity: usize,
    /// 音符 Bind group（uniform + lane_index 存储）
    note_bind_group: wgpu::BindGroup,
    /// 共享的钢琴卷帘常驻音符缓冲（GPU，按 NoteInstance 布局分段）
    note_source: wgpu::Buffer,
    /// 本帧可见音轨在缓冲中的 (offset, len) 分段（已按 doc track 排序）
    note_segments: Vec<(u32, u32)>,
}

/// 顶点着色器代码
const VERTEX_SHADER: &str = include_str!("shaders/arrangement.wgsl");

/// 初始实例缓冲区大小
const INITIAL_CAPACITY: usize = 4096;

/// lane_index 初始容量（文档音轨数上限的保守值）
const INITIAL_LANE_CAPACITY: usize = 1024;
