//! 视频导出面板**入口**守卫：输出路径为空必须拒绝。
//!
//! 回归背景（2026-11-17 用户实测）：未选择输出路径就点「开始导出」，空串一路
//! 传到 ffmpeg，以 `Error opening output file .`（code -22）退出；帧生成线程
//! 白跑一轮、stderr 刷屏、对话框只显示「管道已结束」。本测试锁定入口行为：
//! **状态零变化 + Warning Toast**（与 CPU 入口闸同一口径）。

use crate::message::{Message, VideoExportAction};
use crate::state::root_state::VideoExportOverlayState;
use lumino_core::storage::config::OnionSkinBackend;

use super::{assert_warning_toast, root_with_backend};

#[test]
fn test_start_export_with_blank_output_path_is_rejected() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);
    // 仅空白同样视为空：ffmpeg 会把它当成非法文件名，而不是「当前目录」。
    root.state.video_export_dialog.basic.output_path = "   ".to_owned();

    root.update(Message::VideoExport(VideoExportAction::StartExport));

    assert_warning_toast(&root, "请先选择视频输出路径");
    assert!(
        matches!(
            root.state.video_export_dialog.basic.overlay,
            VideoExportOverlayState::None
        ),
        "拒绝必须发生在设置 Exporting 之前（不得亮起注定失败的进度覆盖层）"
    );
    assert_eq!(
        root.state.video_export_dialog.basic.progress, 0.0,
        "拒绝路径不得写进度字段"
    );
}
