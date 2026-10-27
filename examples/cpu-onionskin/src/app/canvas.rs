//! 画布像素平移（纯函数，供帧循环与滚动一致性测试共用）。
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
