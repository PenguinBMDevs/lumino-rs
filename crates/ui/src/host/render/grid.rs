//! 网格线和标尺实例生成 — 委托到 lumino-gfx

use crate::host::Host;

impl Host {
    /// 生成网格线实例
    pub(super) fn generate_grid_instances(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
    ) -> Vec<lumino_gfx::GridLineInstance> {
        lumino_gfx::generate_grid_instances(
            viewport_width,
            viewport_height,
            keyboard_width,
            ruler_height,
            scroll_x,
            scroll_y,
            zoom_x,
            zoom_y,
        )
    }

    /// 生成标尺实例
    pub(super) fn generate_ruler_instances(
        &self,
        viewport_width: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        zoom_x: f32,
        _ticks_per_measure: u32,
        _ticks_per_beat: u32,
    ) -> Vec<lumino_gfx::RulerTickInstance> {
        lumino_gfx::generate_ruler_instances(
            viewport_width,
            keyboard_width,
            ruler_height,
            scroll_x,
            zoom_x,
        )
    }
}
