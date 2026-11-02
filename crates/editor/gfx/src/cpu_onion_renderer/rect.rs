//! 帧内像素矩形（脏区几何）：[`CpuOnionUploadRect`]。
//!
//! 纯数据 + 纯几何，不依赖 GPU，因此可以脱离设备单测。
//!
//! # 坐标口径
//!
//! 本类型描述的是**纹理内部的像素区域**（帧内坐标，左上角 + 尺寸），与
//! `CpuOnionFrame::x` / `CpuOnionFrame::y`（四边形在屏幕上的物理像素位置）
//! 完全无关：后者决定「画到屏幕哪儿」，前者决定「重写纹理哪块像素」。
//! 二者绝不能互相换算，否则脏区会随窗口位置漂移。

/// 帧内像素矩形（左上角 + 尺寸，单位：像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CpuOnionUploadRect {
    /// 左上角 X（含）
    pub x: u32,
    /// 左上角 Y（含）
    pub y: u32,
    /// 宽（像素）
    pub width: u32,
    /// 高（像素）
    pub height: u32,
}

impl CpuOnionUploadRect {
    /// 构造矩形。
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 是否为空矩形（宽或高为 0）。
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 面积（像素数）；超大矩形用 `u64` 承载，不溢出。
    #[inline]
    #[must_use]
    pub const fn area(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }

    /// 夹到 `[0, max_width] × [0, max_height]`；完全越界（或为空）返回 `None`。
    ///
    /// 越界部分被**裁剪**而不是平移：局部上传只应影响它覆盖得到的像素。
    #[must_use]
    pub fn clamp_to(self, max_width: u32, max_height: u32) -> Option<Self> {
        let x0 = self.x.min(max_width);
        let y0 = self.y.min(max_height);
        let x1 = self.x.saturating_add(self.width).min(max_width);
        let y1 = self.y.saturating_add(self.height).min(max_height);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    /// 是否**非空且完整**落在 `(width, height)` 帧内（局部上传的准入条件）。
    ///
    /// 部分越界的矩形一律判否：脏区是帧生产者给出的**完整**变更范围，
    /// 越界说明生产者与帧尺寸自相矛盾，此时保守回退全量上传而不是猜。
    #[must_use]
    pub fn is_within(self, width: u32, height: u32) -> bool {
        !self.is_empty() && self.clamp_to(width, height) == Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_onion_upload_rect_empty_and_area() {
        assert!(CpuOnionUploadRect::new(1, 2, 0, 5).is_empty());
        assert!(CpuOnionUploadRect::new(1, 2, 5, 0).is_empty());
        assert!(!CpuOnionUploadRect::new(1, 2, 3, 4).is_empty());
        assert_eq!(CpuOnionUploadRect::new(1, 2, 3, 4).area(), 12);
        assert_eq!(CpuOnionUploadRect::default().area(), 0);
        assert_eq!(
            CpuOnionUploadRect::new(0, 0, u32::MAX, 2).area(),
            u64::from(u32::MAX) * 2,
            "面积必须用 u64 承载，不能溢出 u32"
        );
    }

    #[test]
    fn test_cpu_onion_upload_rect_clamp_clips_overflow() {
        // 右侧/下侧越界：裁剪而不是平移
        let clamped = CpuOnionUploadRect::new(2, 2, 4, 4)
            .clamp_to(4, 4)
            .expect("部分越界应可裁剪");
        assert_eq!(clamped, CpuOnionUploadRect::new(2, 2, 2, 2));

        // 完全在帧内：原样返回
        let inner = CpuOnionUploadRect::new(1, 1, 2, 2);
        assert_eq!(inner.clamp_to(4, 4), Some(inner));

        // 起点越界 / 空矩形 / 饱和溢出：一律 None
        assert_eq!(CpuOnionUploadRect::new(4, 0, 1, 1).clamp_to(4, 4), None);
        assert_eq!(CpuOnionUploadRect::new(0, 4, 1, 1).clamp_to(4, 4), None);
        assert_eq!(CpuOnionUploadRect::new(0, 0, 0, 0).clamp_to(4, 4), None);
        assert_eq!(
            CpuOnionUploadRect::new(u32::MAX, 0, u32::MAX, 1).clamp_to(4, 4),
            None
        );
    }

    #[test]
    fn test_cpu_onion_upload_rect_is_within_is_strict() {
        assert!(CpuOnionUploadRect::new(0, 0, 4, 4).is_within(4, 4));
        assert!(CpuOnionUploadRect::new(1, 1, 2, 2).is_within(4, 4));
        // 部分越界 → 否（脏区必须完整落在帧内，越界说明生产者自相矛盾）
        assert!(!CpuOnionUploadRect::new(2, 2, 4, 4).is_within(4, 4));
        assert!(!CpuOnionUploadRect::new(4, 0, 1, 1).is_within(4, 4));
        // 空矩形 → 否
        assert!(!CpuOnionUploadRect::new(0, 0, 0, 2).is_within(4, 4));
        // 尺寸为 0 的帧 → 否
        assert!(!CpuOnionUploadRect::new(0, 0, 1, 1).is_within(0, 0));
    }
}
