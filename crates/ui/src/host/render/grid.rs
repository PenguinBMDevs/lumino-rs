//! 标尺实例生成 — 委托到 lumino-gfx

use crate::host::Host;

impl Host {
    /// 生成标尺实例
    pub(super) fn generate_ruler_instances(
        &self,
        viewport_width: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        zoom_x: f32,
    ) -> Vec<lumino_gfx::RulerTickInstance> {
        let es = &self.root.editor.editor_state;
        let ppq = es.view.ppq;
        let time_signatures = &es.data.time_signatures;
        lumino_gfx::generate_ruler_instances(
            viewport_width,
            keyboard_width,
            ruler_height,
            scroll_x,
            zoom_x,
            ppq as u32,
            time_signatures,
        )
    }
}
