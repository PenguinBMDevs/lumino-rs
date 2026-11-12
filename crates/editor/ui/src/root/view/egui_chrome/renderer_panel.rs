//! 渲染器入口面板：生产状态 → egui 模型。
//!
//! 几何来自 `root::video_clip_math::renderer_panel_layout`（与时间轴 / 预览同一份），
//! 文案与导出设置摘要在此装配；`ui-egui::renderer_panel` 只负责画。

use lumino_core::{tr, trf};
use lumino_ui_egui::renderer_panel::{
    RendererPanelModel, RendererPreviewGate, RendererSettingLine,
};

use crate::root::Root;
use crate::root::video_clip_math::{PreviewGate, preview_gate, renderer_panel_layout};

impl Root {
    /// 构建渲染器入口面板模型；当前不是渲染器入口时返回 `None`。
    pub(crate) fn renderer_panel_model(&self, area: egui::Rect) -> Option<RendererPanelModel> {
        if !self.is_renderer_entry_active() {
            return None;
        }
        let layout = renderer_panel_layout(area);
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            tracing::warn!(?layout, "DEBUG renderer panel");
        }
        let texture_ready = self.waterfall_player.view.is_some();
        let in_band = self
            .state
            .video_clip
            .position_in_video_window(self.clip_real_duration_secs() as f32);
        let (gate, preview_text) = match preview_gate(in_band, texture_ready) {
            PreviewGate::Waterfall => (RendererPreviewGate::Waterfall, String::new()),
            PreviewGate::NoMaterial => (
                RendererPreviewGate::NoMaterial,
                tr!("播放指示线不在视频带上（当前位置无素材）").to_string(),
            ),
            PreviewGate::WaitingRender => (
                RendererPreviewGate::WaitingRender,
                tr!("瀑布流预览（等待渲染…）").to_string(),
            ),
        };

        let export = &self.state.video_export_dialog;
        let settings_lines = vec![
            RendererSettingLine {
                text: tr!("导出设置（复用）").to_string(),
                size: 14.0,
                muted: false,
            },
            RendererSettingLine {
                text: trf!(
                    "格式: {container}  编码: {codec}  质量: {quality}",
                    container = export.container,
                    codec = export.codec,
                    quality = lumino_core::i18n::tr_dyn(&export.quality)
                ),
                size: 12.0,
                muted: true,
            },
            RendererSettingLine {
                text: trf!(
                    "分辨率: {width}x{height}  帧率: {fps}  速度: {speed}x",
                    width = export.width,
                    height = export.height,
                    fps = export.fps,
                    speed = format!("{:.1}", export.waterfall_speed)
                ),
                size: 12.0,
                muted: true,
            },
        ];

        Some(RendererPanelModel {
            left: layout.left,
            header: layout.header,
            preview_wrapper: layout.preview_wrapper,
            preview: layout.preview,
            timeline: layout.timeline,
            settings: layout.settings,
            gate,
            preview_text,
            title: tr!("视频剪辑").to_string(),
            zoom: trf!(
                "缩放 {zoom}x",
                zoom = format!("{:.1}", self.state.video_clip.zoom)
            ),
            left_title: tr!("轨道").to_string(),
            left_placeholder: tr!("（轨道列表占位）").to_string(),
            settings_lines,
        })
    }
}
