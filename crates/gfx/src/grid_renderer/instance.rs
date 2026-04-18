//! GridLineInstance 类型定义（已废弃 CPU 实例生成逻辑）

use bytemuck::{Pod, Zeroable};

/// 兼容旧代码的占位符（已废弃 CPU 实例生成逻辑，保留类型以减少大面积联级修改）
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GridLineInstance {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub color: [f32; 4],
    pub width: f32,
    pub _padding: [f32; 3],
}

impl GridLineInstance {
    pub fn new(start: [f32; 2], end: [f32; 2], color: [f32; 4], width: f32) -> Self {
        Self {
            start,
            end,
            color,
            width,
            _padding: [0.0; 3],
        }
    }
}
