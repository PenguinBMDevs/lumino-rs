//! 总览密度 LOD 的整帧渲染入口。

use crate::{camera::CanvasCamera, compose, density::DensityPyramid};

use super::CpuOnionRenderer;

impl CpuOnionRenderer {
    /// 用密度金字塔重画整帧（透明背景）。
    ///
    /// 本函数**逐行覆盖整帧**（无密度处写透明），调用方无需预先清屏；输出帧也由
    /// `compose::write_output` 全量重写。
    ///
    /// `estimate` 是诊断用可见音符估算（不参与档位判定），写入 `visible_notes`。
    pub(crate) fn render_overview_full(
        &mut self,
        camera: &CanvasCamera,
        density: &DensityPyramid,
        estimate: u64,
    ) {
        let width = self.width;
        let height = self.height;
        if width > 0 && height > 0 {
            density.render_overview(&mut self.frame, width, height, camera);
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
        self.visible_notes = estimate;
    }
}
