//! 彩色覆盖 LOD 的整帧渲染（从 `frame` 拆出，守住单文件 ≤600 行）。

use super::App;
use crate::{
    camera::Camera,
    render::{RenderStats, density},
};

impl App {
    /// 进入全量帧前把密度/覆盖层修到最新：优先脏区局部重建，失败才整建。
    ///
    /// 与生产 `DensityCache::ensure` 的差别：生产只在 `simplified`（本帧确实要用
    /// 覆盖/热力图）分支里调用，示例没有先算档位，因此在 Exact 档编辑后也会做
    /// 一次脏区重建。这是一次便宜的增量写回，不是全量重建；刻意保留，避免示例
    /// 多一条与生产不同步的档位判定（T-32c-1 备注）。
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
