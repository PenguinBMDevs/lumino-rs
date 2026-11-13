//! 视口几何（物理尺寸 + 缩放因子 → 逻辑尺寸）。
//!
//! 与 iced `graphics::Viewport` 同语义的最小实现：只保留本项目用到的四个访问器，
//! 移除 `Transformation` 依赖（离屏渲染参数自己算投影）。

/// 视口：物理像素尺寸与 DPI 缩放。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Viewport {
    physical_size: iced_core::Size<u32>,
    logical_size: iced_core::Size<f32>,
    scale_factor: f32,
}

impl Viewport {
    /// 由物理尺寸与缩放因子构造（`logical = physical / scale`）。
    pub fn with_physical_size(size: iced_core::Size<u32>, scale_factor: f32) -> Viewport {
        Viewport {
            physical_size: size,
            logical_size: iced_core::Size::new(
                size.width as f32 / scale_factor,
                size.height as f32 / scale_factor,
            ),
            scale_factor,
        }
    }

    /// 物理尺寸。
    pub fn physical_size(&self) -> iced_core::Size<u32> {
        self.physical_size
    }

    /// 逻辑尺寸。
    pub fn logical_size(&self) -> iced_core::Size<f32> {
        self.logical_size
    }

    /// DPI 缩放因子。
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}
