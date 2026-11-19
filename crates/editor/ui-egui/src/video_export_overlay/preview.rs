//! 视频导出覆盖层：预览帧纹理上传、等比铺放与高度收缩。

use egui::{Color32, CornerRadius, Vec2};

use crate::chrome::VideoPreviewFrame;
use crate::theme;

use super::Row;

/// 预览占位高度（无帧时）。
pub(super) const PREVIEW_H: f32 = 160.0;
/// 预览帧显示上限（与 上一代 同口径：480×240，不放大）。
const PREVIEW_MAX_W: f32 = 480.0;
const PREVIEW_MAX_H: f32 = 240.0;
/// 预览帧显示下限（避免极小帧缩成一条线）。
const PREVIEW_MIN_H: f32 = 56.0;

/// 预览帧纹理缓存：同一份 RGBA 只上传一次。
#[derive(Clone)]
struct PreviewTexture {
    /// 帧数据（持有 Arc 防止地址复用导致误判）。
    rgba: std::sync::Arc<Vec<u8>>,
    /// 已上传纹理。
    texture: egui::TextureHandle,
}

/// 画预览行：占位框 + 有帧时铺纹理。
pub(super) fn show_row(ui: &egui::Ui, row_rect: egui::Rect, frame: Option<&VideoPreviewFrame>) {
    ui.painter().rect(
        row_rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        egui::Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let Some(frame) = frame else {
        return;
    };
    let texture = preview_texture(ui, frame);
    let image_rect = image_rect(row_rect, frame);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter()
        .image(texture.id(), image_rect, uv, Color32::WHITE);
}

/// 预览行高度：有帧时按 480×240 等比缩放（不放大）；空态固定占位高。
pub(super) fn preview_height(preview: Option<&VideoPreviewFrame>, max_w: f32) -> f32 {
    let Some(frame) = preview else {
        return PREVIEW_H;
    };
    let (w, h) = (frame.width as f32, frame.height as f32);
    if w <= 0.0 || h <= 0.0 {
        return PREVIEW_H;
    }
    let max_w = max_w.min(PREVIEW_MAX_W);
    let scale = (max_w / w).min(PREVIEW_MAX_H / h).min(1.0);
    (h * scale).clamp(PREVIEW_MIN_H, PREVIEW_MAX_H)
}

/// 计划总高超出可用区时压缩预览行（信息行 / 按钮不允许裁）。
pub(super) fn shrink_to_fit(rows: &mut [Row], available: f32) -> f32 {
    let total: f32 = rows.iter().map(Row::height).sum();
    if total <= available {
        return total;
    }
    let Some(height) = rows.iter_mut().find_map(|row| match row {
        Row::Preview(h) => Some(h),
        _ => None,
    }) else {
        return total;
    };
    let target = (*height - (total - available)).max(PREVIEW_MIN_H);
    let saved = *height - target;
    *height = target;
    total - saved
}

/// 帧在预览行内等比铺放（不放大，居中）。
fn image_rect(row_rect: egui::Rect, frame: &VideoPreviewFrame) -> egui::Rect {
    let w = frame.width.max(1) as f32;
    let h = frame.height.max(1) as f32;
    let scale = (row_rect.width() / w).min(row_rect.height() / h).min(1.0);
    egui::Rect::from_center_size(
        row_rect.center(),
        Vec2::new((w * scale).max(1.0), (h * scale).max(1.0)),
    )
}

/// 取预览纹理：同一份帧数据只上传一次。
fn preview_texture(ui: &egui::Ui, frame: &VideoPreviewFrame) -> egui::TextureHandle {
    let cache_id = egui::Id::unique("lumino.video-export-overlay.preview");
    let cached = ui
        .ctx()
        .data_mut(|d| d.get_temp::<PreviewTexture>(cache_id));
    if let Some(cache) = cached.as_ref()
        && std::sync::Arc::ptr_eq(&cache.rgba, &frame.rgba)
    {
        return cache.texture.clone();
    }
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    );
    let texture = ui.ctx().load_texture(
        "lumino-video-export-overlay-preview",
        image,
        egui::TextureOptions::LINEAR,
    );
    let entry = PreviewTexture {
        rgba: std::sync::Arc::clone(&frame.rgba),
        texture,
    };
    ui.ctx()
        .data_mut(|d| d.insert_temp(cache_id, entry.clone()));
    entry.texture
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use super::super::{Action, ButtonKind};

    fn frame(width: u32, height: u32) -> VideoPreviewFrame {
        VideoPreviewFrame {
            width,
            height,
            rgba: Arc::new(vec![0; width as usize * height as usize * 4]),
        }
    }

    #[test]
    fn preview_height_fits_16x9_into_240() {
        // 480×270 按高 240 缩放（宽度 426.7），不撑破旧的 480×240 上限。
        assert!((preview_height(Some(&frame(480, 270)), 520.0) - 240.0).abs() < 0.01);
    }

    #[test]
    fn preview_height_does_not_upscale_small_frames() {
        assert!((preview_height(Some(&frame(100, 60)), 520.0) - 60.0).abs() < 0.01);
    }

    #[test]
    fn preview_height_is_fixed_when_empty() {
        assert!((preview_height(None, 520.0) - PREVIEW_H).abs() < 0.01);
    }

    #[test]
    fn preview_image_rect_keeps_aspect_ratio() {
        let row = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(520.0, 240.0));
        let rect = image_rect(row, &frame(480, 270));
        assert!((rect.width() / rect.height() - 480.0 / 270.0).abs() < 0.01);
        assert!((rect.height() - 240.0).abs() < 0.01);
        assert!((rect.center().x - row.center().x).abs() < 0.01);
        assert!((rect.center().y - row.center().y).abs() < 0.01);
    }

    #[test]
    fn overflow_shrinks_preview_before_information_rows() {
        let mut rows = vec![
            Row::Text("标题".to_owned(), 16.0, Color32::WHITE, true),
            Row::Preview(240.0),
            Row::Button("确定".to_owned(), ButtonKind::Wide, Action::Dismiss),
        ];
        let total = shrink_to_fit(&mut rows, 200.0);
        assert!(total <= 200.0, "压缩后总高必须落进可用区：{total}");
        let preview = rows
            .iter()
            .find_map(|row| match row {
                Row::Preview(h) => Some(*h),
                _ => None,
            })
            .expect("应有预览行");
        assert!((preview - 142.0).abs() < 0.01, "预览行应被压缩：{preview}");
    }
}
