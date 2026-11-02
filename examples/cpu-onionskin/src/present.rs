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
    width: u32,
    height: u32,
    /// Windows 快速路径：直接 `StretchDIBits(Renderer.frame)`，省掉
    /// softbuffer DIB 的 8MB 整帧拷贝。
    #[cfg(target_os = "windows")]
    win32: Option<Win32Surface>,
}

#[cfg(target_os = "windows")]
struct Win32Surface {
    hwnd: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
}

#[cfg(target_os = "windows")]
impl Drop for Win32Surface {
    fn drop(&mut self) {
        // SAFETY: dc 来自 GetDC(hwnd)，只释放一次；窗口仍在 Presenter 生命周期内。
        unsafe {
            windows::Win32::Graphics::Gdi::ReleaseDC(Some(self.hwnd), self.dc);
        }
    }
}

impl Presenter {
    /// 创建空呈现器。
    #[must_use]
    pub fn new() -> Self {
        Self {
            context: None,
            surface: None,
            width: 0,
            height: 0,
            #[cfg(target_os = "windows")]
            win32: None,
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
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(surface) = &mut self.surface {
            let _ = surface.resize(
                NonZeroU32::new(width.max(1)).unwrap_or(NonZeroU32::MIN),
                NonZeroU32::new(height.max(1)).unwrap_or(NonZeroU32::MIN),
            );
        }
    }

    /// 全窗呈现。
    pub fn present(&mut self, window: &Window, frame: &[u32]) {
        #[cfg(target_os = "windows")]
        if self.present_win32(window, frame) {
            return;
        }
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let debug = std::env::var_os("CPU_ONIONSKIN_PRESENT_DEBUG").is_some();
        let t0 = std::time::Instant::now();
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let t1 = std::time::Instant::now();
        if buffer.len() != frame.len() {
            return;
        }
        buffer.copy_from_slice(frame);
        let t2 = std::time::Instant::now();
        window.pre_present_notify();
        let t3 = std::time::Instant::now();
        if let Err(error) = buffer.present() {
            eprintln!("[warn] softbuffer present 失败: {error}");
        }
        if debug {
            let t4 = std::time::Instant::now();
            eprintln!(
                "[present] buffer={:.3} copy={:.3} pre={:.3} blit={:.3} total={:.3} ms",
                (t1 - t0).as_secs_f64() * 1000.0,
                (t2 - t1).as_secs_f64() * 1000.0,
                (t3 - t2).as_secs_f64() * 1000.0,
                (t4 - t3).as_secs_f64() * 1000.0,
                (t4 - t0).as_secs_f64() * 1000.0
            );
        }
    }

    /// 只更新画布 + 标尺（键盘栏/窗口外框保持不动）。
    ///
    /// 滚动帧不再依赖 `ScrollWindowEx`：窗口内容每次都从 DIB 完整覆盖画布，
    /// 因此不会出现"主区域仍是旧帧、只有窄条更新"的错位。键盘栏是静态的，
    /// 首次全窗 present 后不需要每帧重绘。
    pub fn present_canvas(&mut self, window: &Window, frame: &[u32], camera: &Camera) {
        #[cfg(target_os = "windows")]
        if self.present_win32(window, frame) {
            return;
        }
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

    /// Windows 快速呈现：`StretchDIBits` 直接从 Renderer 的 CPU 帧读像素。
    ///
    /// softbuffer 的 Windows 后端同样是 DIB + BitBlt，但要求先
    /// `copy_from_slice` 把 8MB 帧拷进它的 DIB；这里省掉这次拷贝。
    #[cfg(target_os = "windows")]
    fn present_win32(&mut self, window: &Window, frame: &[u32]) -> bool {
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, SRCCOPY, StretchDIBits,
        };

        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 || frame.len() != width as usize * height as usize {
            return false;
        }
        if self.win32.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let Ok(handle) = window.window_handle() else {
                return false;
            };
            let RawWindowHandle::Win32(win32) = handle.as_raw() else {
                return false;
            };
            let hwnd = windows::Win32::Foundation::HWND(win32.hwnd.get() as *mut _);
            // SAFETY: hwnd 来自 winit 的有效窗口句柄；DC 在 Win32Surface Drop 中释放。
            let dc = unsafe { GetDC(Some(hwnd)) };
            if dc.is_invalid() {
                return false;
            }
            self.win32 = Some(Win32Surface { hwnd, dc });
        }
        let Some(surface) = self.win32.as_ref() else {
            return false;
        };
        let debug = std::env::var_os("CPU_ONIONSKIN_PRESENT_DEBUG").is_some();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        window.pre_present_notify();
        let started = std::time::Instant::now();
        // SAFETY: frame 长度按 width*height 校验；BITMAPINFO 描述同一尺寸的
        // 32bpp 自顶向下 DIB，StretchDIBits 只读取 frame 的像素。
        let copied = unsafe {
            StretchDIBits(
                surface.dc,
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                Some(frame.as_ptr().cast()),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            )
        };
        if debug {
            eprintln!(
                "[present-win32] blit={:.3} ms copied={copied}",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        copied != 0
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
