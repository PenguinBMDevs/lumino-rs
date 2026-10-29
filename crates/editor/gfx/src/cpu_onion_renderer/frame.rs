//! CPU 洋葱皮帧：[`CpuOnionFrame`] 数据载体、脏区与尺寸校验。

use std::{fmt, sync::Arc};

use super::rect::CpuOnionUploadRect;

/// 一帧 CPU 光栅洋葱皮（透明背景叠加层）。
///
/// 约定：
/// - `pixels` 为行优先 RGBA8，长度 = `width * height * 4`；
/// - 像素为 **sRGB 编码**值，透明背景 `[0, 0, 0, 0]`，音符 alpha = 255；
/// - `x` / `y` 为物理像素左上角，**不做 DPI 缩放**（调用方负责换算）；
/// - `version` 标识像素内容版本：版本相同视为同一份内容，渲染侧不重复上传；
/// - `dirty_rect` 为**帧内像素坐标**的脏区：`Some` 表示「相对上一帧只有该矩形
///   变化」，渲染侧可只重写该子矩形；`None` 表示整帧重绘（必须全量上传）。
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
    /// 脏区（帧内像素坐标）：`Some` = 相对上一帧只有该矩形变化；`None` = 整帧重绘
    pub dirty_rect: Option<CpuOnionUploadRect>,
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
            // 默认整帧语义：调用方确认是区域重绘时再用 `with_dirty_rect` 标注
            dirty_rect: None,
        }
    }

    /// 标注脏区（帧内像素坐标）并返回自身（链式构造）。
    ///
    /// 只描述「相对上一帧变化了哪块像素」，不改变像素内容与版本；
    /// 传入 `None` 表示整帧重绘语义。
    #[must_use]
    pub fn with_dirty_rect(mut self, rect: Option<CpuOnionUploadRect>) -> Self {
        self.dirty_rect = rect;
        self
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

    /// 区域像素在 `pixels` 中的字节跨度 `(起点, 结束)`，供局部上传切片。
    ///
    /// 语义与 `queue.write_texture` 的源数据要求一致：行距 = `width * 4`，
    /// 共 `height` 行，最后一行只取 `width` 个像素（区域宽）。因此
    ///
    /// ```text
    /// start = (rect.y * width + rect.x) * 4
    /// end   = start + (rect.height - 1) * width * 4 + rect.width * 4
    /// ```
    ///
    /// 以下情况返回 `None`（**不 panic**）：
    /// - 矩形为空、部分越界、尺寸相乘溢出；
    /// - `pixels` 长度不足以容纳该跨度（帧自相矛盾）。
    #[must_use]
    pub fn region_span(&self, rect: CpuOnionUploadRect) -> Option<(usize, usize)> {
        if !rect.is_within(self.width, self.height) {
            return None;
        }
        let stride = (self.width as usize).checked_mul(4)?;
        let row_bytes = (rect.width as usize).checked_mul(4)?;
        let start = (rect.y as usize)
            .checked_mul(stride)?
            .checked_add((rect.x as usize).checked_mul(4)?)?;
        let end = start
            .checked_add((rect.height as usize).checked_sub(1)?.checked_mul(stride)?)?
            .checked_add(row_bytes)?;
        (end <= self.pixels.len()).then_some((start, end))
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
            .field("dirty_rect", &self.dirty_rect)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::CpuOnionUploadRect;
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

    #[test]
    fn test_cpu_onion_frame_dirty_rect_defaults_to_none() {
        let frame = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; 16], 1);
        assert_eq!(
            frame.dirty_rect, None,
            "未标注脏区时必须默认整帧语义（现有构造点无需改动）"
        );

        let dirty = CpuOnionUploadRect::new(1, 0, 1, 2);
        let frame = frame.with_dirty_rect(Some(dirty));
        assert_eq!(
            frame.dirty_rect,
            Some(dirty),
            "with_dirty_rect 必须写入脏区"
        );
        assert_eq!(
            frame.with_dirty_rect(None).dirty_rect,
            None,
            "with_dirty_rect(None) 必须清回整帧语义"
        );
    }

    #[test]
    fn test_cpu_onion_frame_debug_shows_dirty_summary_only() {
        let dirty = CpuOnionUploadRect::new(1, 1, 2, 1);
        let frame = CpuOnionFrame::new(0, 0, 4, 4, vec![200u8; 64], 3).with_dirty_rect(Some(dirty));
        let text = format!("{frame:?}");
        assert!(
            text.contains("dirty_rect: Some("),
            "Debug 必须包含脏区摘要：{text}"
        );
        assert!(
            !text.contains("200"),
            "Debug 不得打印像素内容（整帧可达数 MB）：{text}"
        );
    }

    #[test]
    fn test_cpu_onion_frame_region_span_covers_exact_bytes() {
        let frame = CpuOnionFrame::new(0, 0, 4, 4, vec![0u8; 64], 1);
        let stride = 4 * 4;
        // 单行子矩形：起点 = (y*width + x)*4
        assert_eq!(
            frame.region_span(CpuOnionUploadRect::new(1, 1, 2, 1)),
            Some((stride + 4, stride + 4 + 2 * 4))
        );
        // 最后一行整宽：刚好贴到像素缓冲末尾（不得越界）
        assert_eq!(
            frame.region_span(CpuOnionUploadRect::new(0, 3, 4, 1)),
            Some((3 * stride, 4 * stride))
        );
        // 右下角 1 像素
        assert_eq!(
            frame.region_span(CpuOnionUploadRect::new(3, 3, 1, 1)),
            Some((3 * stride + 3 * 4, 4 * stride))
        );
        // 整帧
        assert_eq!(
            frame.region_span(CpuOnionUploadRect::new(0, 0, 4, 4)),
            Some((0, 4 * stride))
        );
    }

    #[test]
    fn test_cpu_onion_frame_region_span_rejects_invalid() {
        let frame = CpuOnionFrame::new(0, 0, 4, 4, vec![0u8; 64], 1);
        for rect in [
            CpuOnionUploadRect::new(0, 0, 0, 1),
            CpuOnionUploadRect::new(1, 1, 0, 0),
            // 部分越界
            CpuOnionUploadRect::new(2, 2, 4, 4),
            CpuOnionUploadRect::new(4, 0, 1, 1),
            // 饱和溢出
            CpuOnionUploadRect::new(0, 0, u32::MAX, 1),
        ] {
            assert_eq!(
                frame.region_span(rect),
                None,
                "非法区域必须返回 None（不得 panic、不得越界切片）：{rect:?}"
            );
        }

        // 帧自相矛盾（像素不足）：跨度越界也必须 None
        let short = CpuOnionFrame::new(0, 0, 4, 4, vec![0u8; 16], 1);
        assert_eq!(
            short.region_span(CpuOnionUploadRect::new(3, 3, 1, 1)),
            None,
            "像素长度不足时必须拒绝，而不是越界切片"
        );
    }
}
