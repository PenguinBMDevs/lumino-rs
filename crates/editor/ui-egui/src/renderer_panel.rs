//! 渲染器入口面板（视频剪辑窗口）：轨道占位 + 16:9 预览 + 时间轴卡片 + 导出设置。
//!
//! 取代原 iced `view/video_clip/*` 绘制层：几何由
//! `lumino-ui::root::video_clip_math::renderer_panel_layout` 单源计算，本模块只画。
//! 时间轴内部（传输按钮 / 素材带 / 滚动条）继续由 `video_clip_timeline` 叠加绘制。

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind};

use crate::theme;

/// 预览区显示状态（与生产 `PreviewGate` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererPreviewGate {
    /// 纹理就绪且播放头在素材带内：铺离屏纹理。
    Waterfall,
    /// 播放头不在素材带上。
    NoMaterial,
    /// 在素材带上但纹理尚未就绪。
    WaitingRender,
}

/// 导出设置卡的一行。
#[derive(Debug, Clone, PartialEq)]
pub struct RendererSettingLine {
    /// 文本。
    pub text: String,
    /// 字号。
    pub size: f32,
    /// 是否弱色。
    pub muted: bool,
}

/// 渲染器入口面板模型。
#[derive(Debug, Clone, PartialEq)]
pub struct RendererPanelModel {
    /// 左侧轨道占位卡。
    pub left: Rect,
    /// 标题行。
    pub header: Rect,
    /// 16:9 预览外层（base 底色）。
    pub preview_wrapper: Rect,
    /// 16:9 预览黑盒（纹理 / 占位文字）。
    pub preview: Rect,
    /// 时间轴卡片。
    pub timeline: Rect,
    /// 导出设置卡。
    pub settings: Rect,
    /// 预览状态。
    pub gate: RendererPreviewGate,
    /// 预览占位文字（`gate == Waterfall` 时为空）。
    pub preview_text: String,
    /// 标题（「视频剪辑」）。
    pub title: String,
    /// 右上角缩放文本。
    pub zoom: String,
    /// 左侧卡标题（「轨道」）。
    pub left_title: String,
    /// 左侧卡占位说明。
    pub left_placeholder: String,
    /// 导出设置行。
    pub settings_lines: Vec<RendererSettingLine>,
}

/// 绘制整块渲染器入口面板（时间轴内部由其后的 `video_clip_timeline` 叠加）。
pub fn show(ui: &egui::Ui, model: &RendererPanelModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    let painter = ui.painter();

    // 左侧轨道占位卡：weakest 底 + 1px 边 + 圆角 6。
    painter.rect(
        model.left,
        CornerRadius::same(6),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    painter.text(
        model.left.min + egui::vec2(12.0, 12.0),
        Align2::LEFT_TOP,
        &model.left_title,
        FontId::proportional(12.0),
        theme::text_muted(),
    );
    painter.text(
        model.left.min + egui::vec2(12.0, 32.0),
        Align2::LEFT_TOP,
        &model.left_placeholder,
        FontId::proportional(11.0),
        theme::text_on_weak(),
    );

    // 标题行：左侧标题 18，右侧缩放 12。
    painter.text(
        egui::pos2(model.header.left(), model.header.center().y),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(18.0),
        theme::text(),
    );
    painter.text(
        egui::pos2(model.header.right(), model.header.center().y),
        Align2::RIGHT_CENTER,
        &model.zoom,
        FontId::proportional(12.0),
        theme::text_muted(),
    );

    // 预览：外层 base 底，内层黑盒 + 1px 边 + 圆角 6。
    painter.rect_filled(model.preview_wrapper, 0, theme::base_bg());
    painter.rect(
        model.preview,
        CornerRadius::same(6),
        Color32::BLACK,
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );
    if model.gate != RendererPreviewGate::Waterfall {
        painter.text(
            model.preview.center(),
            Align2::CENTER_CENTER,
            &model.preview_text,
            FontId::proportional(13.0),
            theme::text_muted(),
        );
    }

    // 时间轴卡片：内部由 `video_clip_timeline` 叠加。
    painter.rect(
        model.timeline,
        CornerRadius::same(6),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );

    // 导出设置卡：base 底，三行文本（14 / 12 / 12）。
    painter.rect_filled(model.settings, 0, theme::base_bg());
    let mut y = model.settings.top() + 8.0;
    for line in &model.settings_lines {
        let color = if line.muted {
            theme::text_muted()
        } else {
            theme::text_on_weak()
        };
        painter.text(
            egui::pos2(model.settings.left() + 8.0, y),
            Align2::LEFT_TOP,
            &line.text,
            FontId::proportional(line.size),
            color,
        );
        y += line.size + 6.0;
    }

    // 面板是静态展示，不占用输入（时间轴自己登记占用）。
}
