//! CPU 洋葱皮相机换算：逻辑坐标 → **物理像素**（与 GPU 主轨实际屏幕坐标同口径）。
//!
//! [`CanvasCamera`] 只描述**网格内容区**（不含键盘栏 / 标尺），其宽高/滚动/缩放
//! 都是**物理像素**口径；而编辑器状态（`canvas.offset/size`、`view.keyboard_width`、
//! `view.ruler_height`、`scroll`、`zoom`）是**逻辑像素**口径，必须乘 DPI 缩放因子
//! 才能与 GPU 主轨对齐。
//!
//! # ⚠️ `canvas` 是**整块画布**，不是网格内容区
//!
//! `EditorState.canvas.offset/size` 覆盖「左侧键盘栏 + 顶部标尺 + 网格」整块区域
//! （`note_update.rs` 的点击判定必须先减 `keyboard_width` / `ruler_height` 才落在
//! 网格里）。GPU 网格的内容区起点是 `canvas.offset + (keyboard_width, ruler_height)`：
//!
//! ```text
//! world_tick = (screen_x - keyboard_width - canvas_offset.x + scroll_x) / zoom_x
//! key        = max_key - (screen_y - ruler_height - canvas_offset.y + scroll_y) / zoom_y
//! ```
//!
//! 因此 CPU 帧矩形必须是**内容区矩形**（而不是整块 canvas），否则 CPU 音符会相对
//! GPU 网格整体左移 `keyboard_width`、上移 `ruler_height`：
//!
//! ```text
//! x        = floor((canvas.offset_x + keyboard_width) * scale)
//! y        = floor((canvas.offset_y + ruler_height)  * scale)
//! width    = floor((canvas.size_x   - keyboard_width) * scale)   // 负数先夹到 0
//! height   = floor((canvas.size_y   - ruler_height)  * scale)   // 负数先夹到 0
//! scroll_x = scroll.0 * scale        scroll_y = scroll.1 * scale
//! zoom_x   = zoom.0   * scale        zoom_y   = zoom.1   * scale
//! key_count = view.visible_key_count
//! ```
//!
//! 帧矩形 `(x, y, width, height)` 即 `lumino_gfx::CpuOnionFrame` 的物理矩形，
//! 与渲染通道里 canvas scissor 的取整口径一致（都是 `floor(逻辑值 × scale)`）。

use lumino_cpu_onion::CanvasCamera;

/// CPU 洋葱皮相机的逻辑输入（与 GPU 主轨同一套画布/滚动/缩放口径）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuOnionCameraInput {
    /// DPI 缩放因子（`viewport.scale_factor()`）
    pub scale_factor: f32,
    /// 整块 canvas 在窗口中的逻辑偏移 `(offset_x, offset_y)`（**含**键盘栏与标尺）
    pub canvas_offset: (f32, f32),
    /// 整块 canvas 的逻辑尺寸 `(size_x, size_y)`（**含**键盘栏与标尺）
    pub canvas_size: (f32, f32),
    /// 左侧键盘栏宽（逻辑像素，`view.keyboard_width`）——内容区向右内缩量
    pub keyboard_width: f32,
    /// 顶部标尺高（逻辑像素，`view.ruler_height`）——内容区向下内缩量
    pub ruler_height: f32,
    /// 逻辑滚动 `(scroll_x, scroll_y)`
    pub scroll: (f32, f32),
    /// 逻辑缩放 `(zoom_x, zoom_y)`
    pub zoom: (f32, f32),
    /// 纵轴键数（`visible_key_count`）
    pub key_count: u16,
}

/// 物理像素矩形（网格内容区）+ 画布相机（CPU 帧的完整几何）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuOnionGeometry {
    /// 内容区物理左上角 X
    pub x: u32,
    /// 内容区物理左上角 Y
    pub y: u32,
    /// 内容区宽（物理像素，> 0）
    pub width: u32,
    /// 内容区高（物理像素，> 0）
    pub height: u32,
    /// 画布相机（宽高与 `width` / `height` 一致，坐标是内容区局部坐标）
    pub camera: CanvasCamera,
}

impl CpuOnionCameraInput {
    /// 换算到物理像素；返回 `None` 表示本帧不可绘制（不生成也不叠加 CPU 帧）。
    ///
    /// 退化输入一律返回 `None`（保守跳过，绝不猜测坐标导致错位）：
    /// 缩放因子非正/非有限、键盘栏或标尺宽高非有限或为负、**内容区原点为负**
    /// （画布偏移为负且已超过边栏内缩 = 布局异常）、内容区宽或高为 0
    /// （键盘栏/标尺把画布吃光）、滚动/缩放非有限。
    pub(crate) fn to_physical(self) -> Option<CpuOnionGeometry> {
        let scale = self.scale_factor;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        if !self.keyboard_width.is_finite() || !self.ruler_height.is_finite() {
            return None;
        }
        // 键盘栏/标尺是**正向内缩**：负值说明布局异常，宁可本帧不画
        if self.keyboard_width < 0.0 || self.ruler_height < 0.0 {
            return None;
        }
        // 内容区 = 整块 canvas 去掉左侧键盘栏与顶部标尺（与 GPU 网格同口径）
        let content_x = self.canvas_offset.0 + self.keyboard_width;
        let content_y = self.canvas_offset.1 + self.ruler_height;
        let content_width = self.canvas_size.0 - self.keyboard_width;
        let content_height = self.canvas_size.1 - self.ruler_height;
        if content_width < 0.0 || content_height < 0.0 {
            // 边栏比画布还大：键盘栏/标尺把网格吃光了，不画也不猜
            return None;
        }
        let x = physical_px(content_x, scale)?;
        let y = physical_px(content_y, scale)?;
        let width = physical_px(content_width, scale)?;
        let height = physical_px(content_height, scale)?;
        if width == 0 || height == 0 {
            return None;
        }
        // 相机保持**内容区局部坐标**（内容区左上角 = 局部 (0, 0)）
        let camera = CanvasCamera {
            width,
            height,
            scroll_x: self.scroll.0 * scale,
            scroll_y: self.scroll.1 * scale,
            zoom_x: self.zoom.0 * scale,
            zoom_y: self.zoom.1 * scale,
            key_count: self.key_count,
        };
        if !camera.scroll_x.is_finite()
            || !camera.scroll_y.is_finite()
            || !camera.zoom_x.is_finite()
            || !camera.zoom_y.is_finite()
        {
            return None;
        }
        Some(CpuOnionGeometry {
            x,
            y,
            width,
            height,
            camera,
        })
    }
}

/// 逻辑值 × DPI → 物理像素（向下取整）。
///
/// 负值/非有限值返回 `None`（内容区原点为负说明布局异常，宁可不画也不画错位）。
fn physical_px(logical: f32, scale: f32) -> Option<u32> {
    let scaled = logical * scale;
    if !scaled.is_finite() || scaled < 0.0 {
        return None;
    }
    Some(scaled.floor() as u32)
}

/// 几何/DPI/网格对齐测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "camera/tests.rs"]
mod tests;
