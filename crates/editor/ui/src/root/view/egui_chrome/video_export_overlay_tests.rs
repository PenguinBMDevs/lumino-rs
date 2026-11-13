//! 视频导出进度覆盖层：四态模型差异与动作映射。
//!
//! 背景：旧版 侧的覆盖层视图已删除（整窗只留同尺寸占位），因此
//! `host::measure` 的「状态是否改变布局」不再适用于它。四态覆盖改由本用例
//! 在模型层保证：每种状态都必须产出可区分的可见内容，不能退化成同一份。

use super::*;
use crate::state::root_state::VideoExportOverlayState;
use lumino_ui_egui::chrome::{DialogModel, VideoOverlayView};
use lumino_ui_egui::video_export_overlay::VideoOverlayAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::VideoExport,
    )
}

fn overlay(root: &Root) -> VideoOverlayView {
    let model = root.egui_chrome_model();
    let Some(DialogModel::VideoExportOverlay(panel)) = model.dialog else {
        panic!("视频导出进度覆盖层应由 egui 整窗接管");
    };
    panel.view
}

#[test]
fn every_overlay_state_has_distinct_visible_content() {
    let mut root = dialog_root();
    let state = &mut root.state.video_export_dialog;
    state.fps = 60;
    state.total_frames = 3600;
    state.current_frame = 1512;
    state.progress = 0.42;
    state.render_fps = 57.6;
    state.elapsed_secs = 26.2;

    // ── 导出中 ──
    root.state.video_export_dialog.overlay = VideoExportOverlayState::Exporting;
    let VideoOverlayView::Exporting(exporting) = overlay(&root) else {
        panic!("导出中应映射为 Exporting");
    };
    assert_eq!(exporting.frames_line, "帧: 1512 / 3600");
    assert!(!exporting.time_line.is_empty(), "时间行不能为空");
    assert!((exporting.progress - 0.42).abs() < f32::EPSILON);
    assert_eq!(exporting.percent_line, "42.0%");
    assert!(!exporting.speed_line.is_empty(), "速度行不能为空");

    // ── 封装中：比导出中多出「正在完成编码 / 强制完成」──
    root.state.video_export_dialog.overlay = VideoExportOverlayState::Finalizing;
    let VideoOverlayView::Finalizing(finalizing) = overlay(&root) else {
        panic!("封装中应映射为 Finalizing");
    };
    assert!(!finalizing.finalizing_line.is_empty());
    assert!(!finalizing.force_finish.is_empty());
    assert_eq!(finalizing.progress.percent_line, "42.0%");

    // ── 已完成：统计行必须逐项可见 ──
    root.state.video_export_dialog.overlay = VideoExportOverlayState::Completed {
        total_frames: 3600,
        elapsed_secs: 62.5,
        avg_fps: 57.6,
    };
    let VideoOverlayView::Completed(completed) = overlay(&root) else {
        panic!("已完成应映射为 Completed");
    };
    assert_eq!(completed.stats.len(), 5, "总帧数/时长/总用时/平均速度/倍率");
    assert!(
        completed.stats[0].contains("3600"),
        "总帧数：{:?}",
        completed.stats
    );
    assert!(
        completed.stats[3].contains("57.6"),
        "平均速度：{:?}",
        completed.stats
    );

    // ── 失败：错误详情必须原样带出（含换行与长路径）──
    let detail = "ffmpeg 编码失败（退出码 1）：\n无法写入输出文件 D:\\作品\\output.mp4";
    root.state.video_export_dialog.overlay = VideoExportOverlayState::Error(detail.to_owned());
    let VideoOverlayView::Error(error) = overlay(&root) else {
        panic!("失败应映射为 Error");
    };
    assert_eq!(error.detail, detail);
    assert!(!error.heading.is_empty());
}

#[test]
fn no_overlay_means_no_dialog_model() {
    let mut root = dialog_root();
    root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
    let model = root.egui_chrome_model();
    assert!(
        !matches!(model.dialog, Some(DialogModel::VideoExportOverlay(_))),
        "空闲态不应产出覆盖层模型（设置面板照常显示）"
    );
}

#[test]
fn overlay_actions_map_to_messages() {
    let root = dialog_root();
    let map = |action: VideoOverlayAction| root.video_overlay_action_message(&action);

    assert!(matches!(
        map(VideoOverlayAction::CancelExport),
        Some(crate::Message::VideoExport(
            crate::message::VideoExportAction::CancelExport
        ))
    ));
    assert!(matches!(
        map(VideoOverlayAction::ForceFinish),
        Some(crate::Message::VideoExport(
            crate::message::VideoExportAction::ForceFinish
        ))
    ));
    assert!(matches!(
        map(VideoOverlayAction::Dismiss),
        Some(crate::Message::VideoExport(
            crate::message::VideoExportAction::DismissOverlay
        ))
    ));
}
