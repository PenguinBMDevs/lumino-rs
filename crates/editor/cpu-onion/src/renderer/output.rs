//! 输出帧平移 / 取走 / 重建（`renderer.rs` 的子模块，守住单文件 <400 行）。

use crate::compose;

use super::CpuOnionRenderer;

impl CpuOnionRenderer {
    /// 按整数像素平移当前帧（相机滚动增量同向：`dx > 0` 内容左移）。
    ///
    /// 内部 u32 帧与对外 RGBA8 帧同步平移，保证 `frame()` 在 shift 后、
    /// 窄条渲染前仍与相机一致。`|dx| >= width` 时整帧清空。
    pub fn shift_frame(&mut self, dx: i32) {
        if dx == 0 {
            return;
        }
        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 {
            return;
        }
        let needed = width as usize * height as usize * 4;
        let output_missing = self.output.pixels.len() != needed;
        self.ensure_frame_size();
        compose::shift(&mut self.frame, &mut self.output, width, height, dx);
        if output_missing {
            // `take_output` 后内部帧仍在、输出帧被移走：平移内部帧后必须
            // 从它重建完整输出，否则窄条渲染只会留下新露出的那一列。
            compose::write_output(
                &self.frame,
                width as usize,
                height as usize,
                &mut self.output,
                0,
                width,
                self.simd,
            );
        }
        if dx.unsigned_abs() >= width {
            self.visible_notes = 0;
        }
    }

    /// 输出帧缺失时从内部帧重建；区域/窄条路径在 `take_output` 之后调用。
    pub(super) fn ensure_output_current(&mut self) {
        let needed = self.width as usize * self.height as usize * 4;
        let missing = self.output.width != self.width
            || self.output.height != self.height
            || self.output.pixels.len() != needed;
        self.ensure_frame_size();
        if !missing {
            return;
        }
        compose::write_output(
            &self.frame,
            self.width as usize,
            self.height as usize,
            &mut self.output,
            0,
            self.width,
            self.simd,
        );
    }
}
