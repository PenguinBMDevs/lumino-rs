//! 纯 CPU 呈现层：softbuffer 全窗 blit，以及滚动增量路径的
//! `ScrollWindowEx` + damage blit。
//!
//! 绘制始终发生在 `Renderer` 的 CPU 帧缓冲里；这里只负责把像素送到窗口。
//! 滚动时画布已经由 CPU 平移，窗口侧再用 `ScrollWindowEx` 平移同一区域，
//! 最后只 blit 新露出的窄条和标尺，避免每帧 1600×900 全窗 BitBlt 的
//! 合成器节拍抖动。

use std::num::NonZeroU32;

use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use crate::camera::Camera;

/// 窗口呈现器。
pub struct Presenter {
    hwnd: Option<isize>,
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
}

impl Presenter {
    /// 创建空呈现器。
    #[must_use]
    pub fn new() -> Self {
        Self {
            hwnd: None,
            context: None,
            surface: None,
        }
    }

    /// 绑定窗口与 softbuffer surface。
    pub fn init(
        &mut self,
        window: &std::sync::Arc<Window>,
        context: softbuffer::Context<std::sync::Arc<Window>>,
        surface: softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>,
    ) {
        self.hwnd = window
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                #[cfg(target_os = "windows")]
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                _ => None,
            });
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

    /// 滚动增量呈现：窗口画布平移 `dx`，只 blit 新窄条 + 标尺。
    pub fn present_scrolled(
        &mut self,
        window: &Window,
        frame: &[u32],
        camera: &Camera,
        dx: i32,
        strip_x0: u32,
        strip_x1: u32,
    ) {
        let width = camera.width;
        let height = camera.height;
        let keyboard = camera.keyboard_width.round().max(0.0) as u32;
        let ruler = camera.ruler_height.round().max(0.0) as u32;
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd
            && scroll_window_canvas(hwnd, dx, keyboard, ruler, width, height)
        {
            let mut rects = Vec::with_capacity(2);
            if let (Some(strip_width), Some(strip_height)) = (
                NonZeroU32::new(strip_x1.saturating_sub(strip_x0)),
                NonZeroU32::new(height.saturating_sub(ruler)),
            ) {
                rects.push(softbuffer::Rect {
                    x: strip_x0,
                    y: ruler,
                    width: strip_width,
                    height: strip_height,
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
        }
        let _ = (dx, strip_x0, strip_x1);
        self.present(window, frame);
    }

    /// 只更新画布 + 标尺（键盘栏/窗口外框保持不动）。
    ///
    /// 总览热力图每帧都改画布，但键盘栏/边框是静态的；用 damage 只 blit
    /// 画布区域，减小 DWM 合成压力。
    pub fn present_region(&mut self, window: &Window, frame: &[u32], camera: &Camera) {
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

#[cfg(target_os = "windows")]
fn scroll_window_canvas(
    hwnd_raw: isize,
    dx: i32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
) -> bool {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{SW_INVALIDATE, ScrollWindowEx};
    if dx == 0 || left >= width || top >= height {
        return false;
    }
    let rect = RECT {
        left: left as i32,
        top: top as i32,
        right: width as i32,
        bottom: height as i32,
    };
    let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
    // SAFETY: hwnd 来自当前存活的 winit 窗口，rect 在调用期间有效，只在 UI 线程调用。
    let result = unsafe {
        ScrollWindowEx(
            hwnd,
            dx,
            0,
            Some(&rect as *const RECT),
            Some(&rect as *const RECT),
            None,
            None,
            SW_INVALIDATE,
        )
    };
    result != 0
}
