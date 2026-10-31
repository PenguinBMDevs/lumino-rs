//! 彩色覆盖光栅与总览密度热力图的整帧渲染入口。

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
            density.render_overview_with_scratch(
                &mut self.frame,
                width,
                height,
                camera,
                &mut self.overview_scratch,
            );
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

    /// 用彩色覆盖光栅重画整帧（透明背景）。
    ///
    /// 与热力图一样按屏幕列/行整帧重采样；颜色是真实轨道填充色，当前轨/静音轨
    /// 通过单元内第一、第二候选过滤。`estimate` 仍只作诊断计数。
    pub(crate) fn render_coverage_full(
        &mut self,
        camera: &CanvasCamera,
        coverage: &DensityPyramid,
        estimate: u64,
        current_track: usize,
        muted: &[bool],
    ) {
        let width = self.width;
        let height = self.height;
        if width > 0 && height > 0 {
            // 覆盖光栅逐行覆盖整帧（无覆盖处写透明），不需要额外清屏。
            coverage.render_coverage_with_scratch(
                &mut self.frame,
                width,
                height,
                camera,
                &self.style,
                current_track,
                muted,
                &mut self.overview_scratch,
            );
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
