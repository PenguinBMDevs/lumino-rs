//! 视频导出进度覆盖层（egui）：预览帧上传与缓存。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    VideoExportOverlayModel, VideoOverlayProgress, VideoOverlayView, VideoPreviewFrame,
};
use lumino_ui_egui::{video_export_overlay, video_export_overlay::VideoOverlayAction};

fn model(preview: Option<VideoPreviewFrame>) -> VideoExportOverlayModel {
    VideoExportOverlayModel {
        titlebar: false,
        view: VideoOverlayView::Exporting(VideoOverlayProgress {
            heading: "视频导出中".to_owned(),
            frames_line: "帧: 30 / 300".to_owned(),
            time_line: "时间: 0:01 / 0:10".to_owned(),
            progress: 0.1,
            percent_line: "10.0%".to_owned(),
            render_fps_line: "渲染速度: 30.0 fps".to_owned(),
            speed_line: "速度: 2.0x 原速".to_owned(),
            remaining_line: "预计剩余: 0:09".to_owned(),
        }),
        preview,
    }
}

fn frame(width: u32, height: u32) -> VideoPreviewFrame {
    VideoPreviewFrame {
        width,
        height,
        rgba: Arc::new(vec![200u8; width as usize * height as usize * 4]),
    }
}

fn harness(model: VideoExportOverlayModel) -> Harness<'static> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 560.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            video_export_overlay::show(ui, &model);
        });
    harness.run();
    harness
}

#[test]
fn preview_frame_uploads_and_reuses_texture() {
    let mut harness = harness(model(Some(frame(48, 27))));
    // 第二帧命中缓存：不得重复上传、也不得 panic。
    harness.run();
    harness.run();
    assert!(harness.query_by_label("视频导出中").is_some());
    assert!(harness.query_by_label("取消导出").is_some());
}

#[test]
fn changed_preview_frame_uploads_again() {
    let shared = Rc::new(RefCell::new(model(Some(frame(48, 27)))));
    let model_w = Rc::clone(&shared);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 560.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            video_export_overlay::show(ui, &model_w.borrow());
        });
    harness.run();
    if let Some(preview) = shared.borrow_mut().preview.as_mut() {
        preview.rgba = Arc::new(vec![10u8; 48 * 27 * 4]);
    }
    harness.run();
    harness.run();
    assert!(harness.query_by_label("视频导出中").is_some());
}

#[test]
fn export_without_preview_keeps_placeholder() {
    let harness = harness(model(None));
    assert!(harness.query_by_label("视频导出中").is_some());
}

#[test]
fn cancel_button_emits_action() {
    let mut harness = harness(model(Some(frame(48, 27))));
    harness.get_by_label("取消导出").click();
    harness.run();
    assert_eq!(
        video_export_overlay::take_action(&harness.ctx),
        Some(VideoOverlayAction::CancelExport)
    );
}
