//! 彩色覆盖 LOD 的整帧渲染（从 `frame` 拆出，守住单文件 ≤600 行）。

use super::App;
use crate::{
    camera::Camera,
    render::{RenderStats, density},
};

impl App {
    /// 进入全量帧前把密度/覆盖层修到最新：优先脏区局部重建，失败才整建。
    ///
    /// 与生产 `DensityCache::ensure` 一致：只要覆盖层失效就尝试局部重建，
    /// 不再按缩放预判（缩放判定只决定本帧是否用它）。
    pub(super) fn ensure_density_ready(&mut self) {
        if self.editor.density_dirty {
            self.density = density::DensityPyramid::build(
                &self.document,
                self.workers,
                crate::camera::KEY_COUNT,
            );
            self.editor.density_dirty = false;
            return;
        }
        if !self.density.coverage_valid()
            && !self
                .density
                .rebuild_dirty_coverage(&self.document, &self.index)
        {
            self.density = density::DensityPyramid::build(
                &self.document,
                self.workers,
                crate::camera::KEY_COUNT,
            );
        }
    }

    pub(super) fn render_coverage_frame(
        &mut self,
        camera: &Camera,
        width: usize,
        height: usize,
        estimate: u64,
    ) -> RenderStats {
        let frame = self.renderer.frame_mut();
        density::render_coverage(
            &self.density,
            frame,
            width,
            height,
            camera,
            &self.density_style,
            &mut self.density_scratch,
        );
        RenderStats {
            visible_notes: estimate,
            candidates: 0,
            long_candidates: 0,
            bands: 0,
            per_band: Vec::new(),
        }
    }
}
