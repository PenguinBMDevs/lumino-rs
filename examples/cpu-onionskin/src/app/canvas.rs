//! 画布像素平移（纯函数 + App 包装，供帧循环与滚动一致性测试共用）。

use super::App;

impl App {
    pub(super) fn shift_canvas(&mut self, dx: i32) {
        let width = self.renderer.width() as usize;
        let height = self.renderer.height() as usize;
        if width == 0 || height == 0 {
            return;
        }
        let keyboard = self
            .camera
            .as_ref()
            .map_or(0, |camera| camera.keyboard_width.round().max(0.0) as usize)
            .min(width);
        let frame = self.renderer.frame_mut();
        if frame.len() != width * height {
            return;
        }
        shift_canvas_pixels(frame, width, height, keyboard, dx);
    }
}

pub(crate) fn shift_canvas_pixels(
    frame: &mut [u32],
    width: usize,
    height: usize,
    keyboard: usize,
    dx: i32,
) {
    if width == 0 || height == 0 || keyboard >= width || frame.len() != width * height {
        return;
    }
    if dx > 0 {
        let shift = dx as usize;
        if shift >= width - keyboard {
            return;
        }
        for y in 0..height {
            let row = &mut frame[y * width..(y + 1) * width];
            row.copy_within(keyboard + shift..width, keyboard);
        }
    } else {
        let shift = (-dx) as usize;
        if shift == 0 || shift >= width - keyboard {
            return;
        }
        for y in 0..height {
            let row = &mut frame[y * width..(y + 1) * width];
            row.copy_within(keyboard..width - shift, keyboard + shift);
        }
    }
}
