//! CPU 洋葱皮相机换算：逻辑坐标 → **物理像素**（与 GPU 主轨实际屏幕坐标同口径）。
//!
//! [`CanvasCamera`] 只描述 canvas 区域（无键盘栏 / 标尺偏移），其宽高/滚动/缩放
//! 都是**物理像素**口径；而编辑器状态（`canvas.offset/size`、`scroll`、`zoom`）
//! 是**逻辑像素**口径，必须乘 DPI 缩放因子才能与 GPU 主轨对齐：
//!
//! ```text
//! x        = floor(canvas.offset_x * scale)
//! y        = floor(canvas.offset_y * scale)
//! width    = floor(canvas.size_x   * scale)
//! height   = floor(canvas.size_y   * scale)
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
    /// canvas 在窗口中的逻辑偏移 `(offset_x, offset_y)`
    pub canvas_offset: (f32, f32),
    /// canvas 的逻辑尺寸 `(size_x, size_y)`
    pub canvas_size: (f32, f32),
    /// 逻辑滚动 `(scroll_x, scroll_y)`
    pub scroll: (f32, f32),
    /// 逻辑缩放 `(zoom_x, zoom_y)`
    pub zoom: (f32, f32),
    /// 纵轴键数（`visible_key_count`）
    pub key_count: u16,
}

/// 物理像素矩形 + 画布相机（CPU 帧的完整几何）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuOnionGeometry {
    /// 物理像素左上角 X
    pub x: u32,
    /// 物理像素左上角 Y
    pub y: u32,
    /// 宽（物理像素，> 0）
    pub width: u32,
    /// 高（物理像素，> 0）
    pub height: u32,
    /// 画布相机（宽高与 `width` / `height` 一致）
    pub camera: CanvasCamera,
}

impl CpuOnionCameraInput {
    /// 换算到物理像素；返回 `None` 表示本帧不可绘制（不生成也不叠加 CPU 帧）。
    ///
    /// 退化输入一律返回 `None`（保守跳过，绝不猜测坐标导致错位）：
    /// 缩放因子非正/非有限、画布偏移为负、画布宽或高为 0、滚动/缩放非有限。
    pub(crate) fn to_physical(self) -> Option<CpuOnionGeometry> {
        let scale = self.scale_factor;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let x = physical_px(self.canvas_offset.0, scale)?;
        let y = physical_px(self.canvas_offset.1, scale)?;
        let width = physical_px(self.canvas_size.0, scale)?;
        let height = physical_px(self.canvas_size.1, scale)?;
        if width == 0 || height == 0 {
            return None;
        }
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
/// 负值/非有限值返回 `None`（画布偏移为负说明布局异常，宁可不画也不画错位）。
fn physical_px(logical: f32, scale: f32) -> Option<u32> {
    let scaled = logical * scale;
    if !scaled.is_finite() || scaled < 0.0 {
        return None;
    }
    Some(scaled.floor() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 基准输入：画布偏移 (60, 30)、尺寸 (1000, 500)、滚动 (100, 50)、缩放 (2, 20)。
    fn input(scale_factor: f32) -> CpuOnionCameraInput {
        CpuOnionCameraInput {
            scale_factor,
            canvas_offset: (60.0, 30.0),
            canvas_size: (1000.0, 500.0),
            scroll: (100.0, 50.0),
            zoom: (2.0, 20.0),
            key_count: 128,
        }
    }

    /// 100%：物理 = 逻辑（矩形与相机原样）。
    #[test]
    fn physical_geometry_at_100_percent() {
        let geometry = input(1.0).to_physical().expect("100% 应可换算");
        assert_eq!((geometry.x, geometry.y), (60, 30));
        assert_eq!((geometry.width, geometry.height), (1000, 500));
        assert_eq!(geometry.camera.width, 1000);
        assert_eq!(geometry.camera.height, 500);
        assert_eq!(geometry.camera.scroll_x, 100.0);
        assert_eq!(geometry.camera.scroll_y, 50.0);
        assert_eq!(geometry.camera.zoom_x, 2.0);
        assert_eq!(geometry.camera.zoom_y, 20.0);
        assert_eq!(geometry.camera.key_count, 128);
    }

    /// 125%：矩形/滚动/缩放全部 ×1.25，且半像素向下取整（30 × 1.25 = 37.5 → 37）。
    #[test]
    fn physical_geometry_at_125_percent() {
        let geometry = input(1.25).to_physical().expect("125% 应可换算");
        assert_eq!(
            (geometry.x, geometry.y),
            (75, 37),
            "offset_y 37.5 必须向下取整"
        );
        assert_eq!((geometry.width, geometry.height), (1250, 625));
        assert_eq!(geometry.camera.width, 1250);
        assert_eq!(geometry.camera.height, 625);
        assert_eq!(geometry.camera.scroll_x, 125.0);
        assert_eq!(geometry.camera.scroll_y, 62.5);
        assert_eq!(geometry.camera.zoom_x, 2.5);
        assert_eq!(geometry.camera.zoom_y, 25.0);
    }

    /// 150%：矩形/滚动/缩放全部 ×1.5。
    #[test]
    fn physical_geometry_at_150_percent() {
        let geometry = input(1.5).to_physical().expect("150% 应可换算");
        assert_eq!((geometry.x, geometry.y), (90, 45));
        assert_eq!((geometry.width, geometry.height), (1500, 750));
        assert_eq!(geometry.camera.scroll_x, 150.0);
        assert_eq!(geometry.camera.scroll_y, 75.0);
        assert_eq!(geometry.camera.zoom_x, 3.0);
        assert_eq!(geometry.camera.zoom_y, 30.0);
    }

    /// 小数逻辑值同样向下取整（与渲染通道 canvas scissor 的口径一致）。
    #[test]
    fn fractional_logical_values_floor() {
        let mut input = input(1.25);
        input.canvas_offset = (60.7, 30.2);
        input.canvas_size = (1000.9, 500.1);
        let geometry = input.to_physical().expect("小数逻辑值应可换算");
        // 60.7 × 1.25 = 75.875 → 75；30.2 × 1.25 = 37.75 → 37
        assert_eq!((geometry.x, geometry.y), (75, 37));
        // 1000.9 × 1.25 = 1251.125 → 1251；500.1 × 1.25 = 625.125 → 625
        assert_eq!((geometry.width, geometry.height), (1251, 625));
    }

    /// 退化输入一律跳过（不生成错位帧）。
    #[test]
    fn degenerate_inputs_are_skipped() {
        // 缩放因子非法
        for scale in [0.0f32, -1.0, f32::NAN, f32::INFINITY] {
            assert!(
                input(scale).to_physical().is_none(),
                "scale={scale} 必须跳过"
            );
        }
        // 画布宽/高为 0（未布局 / 面板隐藏）
        let mut zero_size = input(1.0);
        zero_size.canvas_size = (0.0, 500.0);
        assert!(zero_size.to_physical().is_none());
        zero_size.canvas_size = (1000.0, 0.0);
        assert!(zero_size.to_physical().is_none());
        // 负画布偏移（布局异常）
        let mut negative = input(1.0);
        negative.canvas_offset = (-1.0, 0.0);
        assert!(negative.to_physical().is_none());
        // 滚动/缩放非有限
        let mut nan_zoom = input(1.0);
        nan_zoom.zoom = (f32::NAN, 20.0);
        assert!(nan_zoom.to_physical().is_none());
        let mut inf_scroll = input(1.0);
        inf_scroll.scroll = (f32::INFINITY, 0.0);
        assert!(inf_scroll.to_physical().is_none());
    }
}
