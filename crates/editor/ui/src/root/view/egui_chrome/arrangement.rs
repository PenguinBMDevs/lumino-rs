//! 工程走带视图：生产状态 → egui 标尺模型。
//!
//! 只在走带路由激活时产出；几何量与 iced `view_arrangement` 同源
//! （`arrangement_view.viewport` + `editor_state`）。

use lumino_ui_egui::chrome::ArrangementRulerModel;

use crate::root::Root;

impl Root {
    /// 走带标尺模型；`None` = 当前不在走带路由。
    pub(crate) fn arrangement_ruler_model(&self) -> Option<ArrangementRulerModel> {
        if !self.sidebar.is_arrangement_route() {
            return None;
        }
        let vp = &self.arrangement_view.viewport;
        // 左缘 = 左侧栏（48 + 面板宽）；顶缘 = 标题栏 + **实际**工具栏高度。
        let left_inset = 48.0
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        let top_inset = if self.settings.synth.use_native_titlebar {
            0.0
        } else {
            lumino_ui_egui::layout::TITLEBAR_H
        } + self.toolbar.height;
        Some(ArrangementRulerModel {
            scroll_x: vp.scroll_x,
            zoom_x: vp.zoom_x,
            ppq: self.editor.editor_state.view.ppq,
            left_inset,
            top_inset,
            time_signatures: self.editor.editor_state.data.time_signatures.clone(),
        })
    }
}
