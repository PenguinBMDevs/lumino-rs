//! 纯 CPU 呈现层：softbuffer + damage blit。
//!
//! 绘制始终发生在 `Renderer` 的 CPU 帧缓冲里；这里只负责把像素送到窗口。
//! 滚动帧把 CPU 帧完整复制进 DIB，然后只 blit 画布 + 标尺。**不再使用
//! `ScrollWindowEx`**：DWM 下窗口表面的平移不可靠，会出现主区域仍是旧帧、
//! 只有窄条更新的错位（新进入视口的音符看起来是坏的）。

use std::num::NonZeroU32;

use winit::window::Window;

use crate::camera::Camera;

/// 窗口呈现器。
pub struct Presenter {
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
}

impl Presenter {
    /// 创建空呈现器。
    #[must_use]
    pub fn new() -> Self {
        Self {
            context: None,
            surface: None,
        }
    }

    /// 绑定窗口与 softbuffer surface。
    pub fn init(
        &mut self,
        context: softbuffer::Context<std::sync::Arc<Window>>,
        surface: softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>,
    ) {
        self.context = Some(context);
        self.surface = Some(surface);
    }

    /// 调整 surface 尺寸。
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(surface) = &mut self.surface {
            let _ = surface.resize(
                NonZeroU32::new(width.max(1)).unwrap_or(NonZeroU32::MIN),
                NonZeroU32::new(height.max(1)).unwrap_or(NonZeroU32::MIN),
            );
        }
    }

    /// 全窗呈现。
    pub fn present(&mut self, window: &Window, frame: &[u32]) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        if buffer.len() != frame.len() {
            return;
        }
        buffer.copy_from_slice(frame);
        window.pre_present_notify();
        if let Err(error) = buffer.present() {
            eprintln!("[warn] softbuffer present 失败: {error}");
        }
    }

    /// 只更新画布 + 标尺（键盘栏/窗口外框保持不动）。
    ///
    /// 滚动帧不再依赖 `ScrollWindowEx`：窗口内容每次都从 DIB 完整覆盖画布，
    /// 因此不会出现"主区域仍是旧帧、只有窄条更新"的错位。键盘栏是静态的，
    /// 首次全窗 present 后不需要每帧重绘。
    pub fn present_canvas(&mut self, window: &Window, frame: &[u32], camera: &Camera) {
        let width = camera.width;
        let height = camera.height;
        let keyboard = camera.keyboard_width.round().max(0.0) as u32;
        let ruler = camera.ruler_height.round().max(0.0) as u32;
        let mut rects = Vec::with_capacity(2);
        if let (Some(region_width), Some(region_height)) = (
            NonZeroU32::new(width.saturating_sub(keyboard)),
            NonZeroU32::new(height.saturating_sub(ruler)),
        ) {
            rects.push(softbuffer::Rect {
                x: keyboard,
                y: ruler,
                width: region_width,
                height: region_height,
            });
        }
        if let (Some(ruler_width), Some(ruler_height)) = (
            NonZeroU32::new(width.saturating_sub(keyboard)),
            NonZeroU32::new(ruler),
        ) {
            rects.push(softbuffer::Rect {
                x: keyboard,
                y: 0,
                width: ruler_width,
                height: ruler_height,
            });
        }
        if !rects.is_empty() && self.present_damage(window, frame, &rects) {
            return;
        }
        self.present(window, frame);
    }

    fn present_damage(
        &mut self,
        window: &Window,
        frame: &[u32],
        rects: &[softbuffer::Rect],
    ) -> bool {
        let Some(surface) = self.surface.as_mut() else {
            return false;
        };
        let Ok(mut buffer) = surface.buffer_mut() else {
            return false;
        };
        if buffer.len() != frame.len() {
            return false;
        }
        buffer.copy_from_slice(frame);
        window.pre_present_notify();
        if let Err(error) = buffer.present_with_damage(rects) {
            eprintln!("[warn] softbuffer damage present 失败: {error}");
            return false;
        }
        true
    }
}
