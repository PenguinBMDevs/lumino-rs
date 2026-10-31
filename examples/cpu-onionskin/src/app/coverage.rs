//! 彩色覆盖 LOD 的整帧渲染（从 `frame` 拆出，守住单文件 <400 行）。

use std::sync::Arc;

use super::App;
use crate::{
    camera::Camera,
    render::{RenderStats, density},
};

impl App {
    /// 进入全量帧前把密度/覆盖层修到最新：优先脏区局部重建，失败才整建。
    pub(super) fn ensure_density_ready(&mut self, camera: &Camera) {
        if self.editor.density_dirty {
            self.density = density::DensityPyramid::build(&self.document, self.workers);
            self.editor.density_dirty = false;
            return;
        }
        if !self.density.coverage_valid()
            && self.density.coverage_applicable(camera)
            && !self
                .density
                .rebuild_dirty_coverage(&self.document, &self.index)
        {
            self.density = density::DensityPyramid::build(&self.document, self.workers);
        }
    }

    pub(super) fn render_coverage_frame(
        &mut self,
        camera: &Camera,
        width: usize,
        height: usize,
        estimate: u64,
    ) -> RenderStats {
        let style = Arc::clone(self.renderer.style());
        let frame = self.renderer.frame_mut();
        self.density.render_coverage(
            frame,
            width,
            height,
            camera,
            &style,
            self.current_track,
            &mut self.overview_scratch,
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
