//! 彩色覆盖 LOD 的整帧渲染（从 `frame` 拆出，守住单文件 <400 行）。

use std::sync::Arc;

use super::App;
use crate::{camera::Camera, render::RenderStats};

impl App {
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
