//! 时间轴标尺渲染器 - 使用 wgpu 实例化渲染高效绘制标尺
//!
//! 替代 iced Canvas 绘制，解决黑乐谱编辑时的性能瓶颈
//!
//! # 模块结构
//!
//! - [`core`] — 核心数据类型与构造方法
//! - [`draw`] — 绘制方法
//! - [`prepare`] — prepare 逻辑（刻度实例生成、缓存、GPU 数据上传）

mod core;
mod draw;
mod prepare;

/// 顶点着色器代码
const VERTEX_SHADER: &str = include_str!("shaders/ruler.wgsl");

/// 初始实例缓冲区大小
const INITIAL_CAPACITY: usize = 4096;

/// 缓冲区扩容因子
const GROWTH_FACTOR: usize = 2;

/// 标尺渲染器
pub struct RulerRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 实例缓冲区
    instance_buffer: wgpu::Buffer,
    /// 视口 uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// 小节线颜色
    measure_color: [f32; 4],
    /// 拍线颜色
    beat_color: [f32; 4],
    /// 细分线颜色
    subdivision_color: [f32; 4],
    /// 背景颜色
    background_color: [f32; 4],
    /// 缓存的刻度实例数据（避免每帧重新生成）
    cached_instances: Vec<RulerTickInstance>,
    /// 缓存是否有效
    cache_valid: bool,
    /// 缓存参数：用于判断是否需要重新生成
    cache_scroll_x: f32,
    cache_zoom_x: f32,
    cache_viewport_width: f32,
    cache_keyboard_width: f32,
    cache_ruler_height: f32,
    cache_ticks_per_measure: u32,
    cache_ticks_per_beat: u32,
}

pub use core::{RulerPrepareParams, RulerTickInstance, RulerViewportUniform};
