//! CPU 洋葱皮帧：[`CpuOnionFrame`] 数据载体与尺寸校验。

use std::{fmt, sync::Arc};

/// 一帧 CPU 光栅洋葱皮（透明背景叠加层）。
///
/// 约定：
/// - `pixels` 为行优先 RGBA8，长度 = `width * height * 4`；
/// - 像素为 **sRGB 编码**值，透明背景 `[0, 0, 0, 0]`，音符 alpha = 255；
/// - `x` / `y` 为物理像素左上角，**不做 DPI 缩放**（调用方负责换算）；
/// - `version` 标识像素内容版本：版本相同视为同一份内容，渲染侧不重复上传。
#[derive(Clone)]
pub struct CpuOnionFrame {
    /// 物理像素左上角 X
    pub x: u32,
    /// 物理像素左上角 Y
    pub y: u32,
    /// 宽度（物理像素）
    pub width: u32,
    /// 高度（物理像素）
    pub height: u32,
    /// 行优先 RGBA8 sRGB 像素（长度必须等于 `width * height * 4`）
    pub pixels: Arc<[u8]>,
    /// 内容版本：变化时重新上传纹理
    pub version: u64,
}

impl CpuOnionFrame {
    /// 构造一帧（像素按 `Arc<[u8]>` 共享，不拷贝像素）。
    ///
    /// # 参数
    /// * `x` / `y` — 物理像素左上角
    /// * `width` / `height` — 物理像素尺寸
    /// * `pixels` — 行优先 RGBA8 sRGB 像素（接受 `Arc<[u8]>` 或 `Vec<u8>`）
    /// * `version` — 内容版本（内容不变时保持同一值，避免重复上传）
    #[must_use]
    pub fn new(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
        version: u64,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            pixels: pixels.into(),
            version,
        }
    }

    /// 期望的像素字节数（`width * height * 4`）；乘法溢出返回 `None`。
    #[must_use]
    pub fn expected_len(width: u32, height: u32) -> Option<usize> {
        (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)
    }

    /// 是否为空帧（宽或高为 0）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 尺寸与像素长度是否自洽。
    ///
    /// 返回 `false` 时渲染侧既不绘制也不上传，且不会 panic——包括宽/高为 0、
    /// `pixels` 长度与 `width * height * 4` 不一致、以及尺寸乘法溢出。
    #[must_use]
    pub fn validate(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        Self::expected_len(self.width, self.height) == Some(self.pixels.len())
    }

    /// `(width, height)` 物理像素尺寸。
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// 自定义 `Debug`：只打印元信息与像素字节数，**不打印像素内容**
///（整帧可达数 MB，逐字节输出会淹没日志）。
impl fmt::Debug for CpuOnionFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpuOnionFrame")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels_len", &self.pixels.len())
            .field("version", &self.version)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_onion_frame_validate_accepts_matching_length() {
        let frame = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; 16], 1);
        assert!(frame.validate(), "2x2 帧的像素长度应为 16 字节");
        assert_eq!(frame.size(), (2, 2));
    }

    #[test]
    fn test_cpu_onion_frame_validate_rejects_length_mismatch() {
        for len in [0usize, 8, 15, 17, 32] {
            let frame = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; len], 1);
            assert!(!frame.validate(), "2x2 帧的 {len} 字节像素长度必须校验失败");
        }
    }

    #[test]
    fn test_cpu_onion_frame_validate_rejects_zero_size() {
        assert!(!CpuOnionFrame::new(0, 0, 0, 0, Vec::new(), 1).validate());
        assert!(!CpuOnionFrame::new(0, 0, 0, 4, Vec::new(), 1).validate());
        assert!(!CpuOnionFrame::new(0, 0, 4, 0, Vec::new(), 1).validate());
        assert!(!CpuOnionFrame::new(0, 0, 1, 1, vec![0u8; 4], 1).is_empty());
    }

    #[test]
    fn test_cpu_onion_frame_expected_len_overflow_is_none() {
        assert_eq!(CpuOnionFrame::expected_len(2, 3), Some(24));
        assert_eq!(CpuOnionFrame::expected_len(0, 0), Some(0));
        assert_eq!(
            CpuOnionFrame::expected_len(u32::MAX, u32::MAX),
            None,
            "超大尺寸的字节数乘法必须溢出为 None，而不是 panic"
        );
    }

    #[test]
    fn test_cpu_onion_frame_debug_hides_pixels() {
        let frame = CpuOnionFrame::new(3, 4, 1, 1, vec![200, 10, 20, 255], 42);
        let text = format!("{frame:?}");
        assert!(text.contains("x: 3"), "Debug 应包含矩形信息：{text}");
        assert!(text.contains("version: 42"), "Debug 应包含版本：{text}");
        assert!(
            text.contains("pixels_len: 4"),
            "Debug 应包含像素字节数：{text}"
        );
        assert!(
            !text.contains("200"),
            "Debug 不得打印像素内容（会淹没日志）：{text}"
        );
    }

    #[test]
    fn test_cpu_onion_frame_clone_shares_pixels() {
        let frame = CpuOnionFrame::new(0, 0, 1, 1, vec![1, 2, 3, 4], 7);
        let cloned = frame.clone();
        assert!(
            Arc::ptr_eq(&frame.pixels, &cloned.pixels),
            "Clone 必须共享像素（Arc 引用计数），不得复制整帧"
        );
        assert_eq!(cloned.version, 7);
        assert_eq!(cloned.size(), (1, 1));
    }
}
