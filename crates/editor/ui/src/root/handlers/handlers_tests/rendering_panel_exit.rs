//! 渲染面板**出口**守卫测试（Step 4b）
//!
//! 入口写一个 `match` 拦住了不算完：**每条能改 `sidebar.route` 的生产路径都是入口**。
//! 关闭音频/视频导出面板的历史落点是 `Route::Arrangement`，CPU 洋葱皮后端下走带
//! 已被入口闸禁用 —— 若不改出口，「打开音频导出再关闭」就是进走带的后门。
//!
//! 断言口径：CPU 下关闭面板后 `route != Arrangement`、`is_arrangement_mode() == false`、
//! 回到可见的钢琴卷帘；GPU 下保持历史落点（零回归）。

use crate::message::{AudioExportAction, Message, VideoExportAction};
use crate::sidebar::{GroupId, Route};
use lumino_core::storage::config::OnionSkinBackend;

use super::root_with_backend;

/// CPU 下音频导出**允许打开**（离线音频渲染与洋葱皮无关），但「关闭」这个出口
/// 不能把用户送进已被入口闸禁用的工程走带——否则「开一下再关」就是后门。
#[test]
fn test_cpu_audio_export_close_does_not_enter_arrangement() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.update(Message::AudioExport(AudioExportAction::OpenPanel));
    assert!(root.sidebar.audio_export_visible, "CPU 下音频导出必须可用");
    assert!(root.toast.is_empty(), "合法入口不得弹拦截 Toast");

    root.update(Message::AudioExport(AudioExportAction::ClosePanel));

    assert!(!root.sidebar.audio_export_visible, "面板必须关闭");
    assert_ne!(
        root.sidebar.route,
        Route::Arrangement,
        "CPU 下关闭音频导出不得落进工程走带（绕过入口闸）"
    );
    assert!(
        !root.is_arrangement_mode(),
        "is_arrangement_mode 必须为 false（渲染层判定同源）"
    );
    assert!(
        root.sidebar.piano_roll_visible,
        "必须回到可见的钢琴卷帘编辑视图，而不是空白区域"
    );
    assert_ne!(
        root.sidebar.active_group,
        Some(GroupId::Project),
        "不得残留走带分组"
    );
}

/// GPU 回归：关闭音频导出仍回到工程走带（历史行为，默认路径零变化）。
#[test]
fn test_gpu_audio_export_close_keeps_arrangement() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.update(Message::AudioExport(AudioExportAction::OpenPanel));
    root.update(Message::AudioExport(AudioExportAction::ClosePanel));

    assert!(!root.sidebar.audio_export_visible);
    assert_eq!(root.sidebar.route, Route::Arrangement, "GPU 下保持历史落点");
    assert!(root.is_arrangement_mode());
}

/// CPU 下视频导出面板本不该打开（入口闸拦了），这里模拟「历史遗留已打开」，
/// 关闭时同样不得落进走带（防御未来/时序绕过）。
#[test]
fn test_cpu_video_export_close_does_not_enter_arrangement() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    // 直接置状态，绕过入口闸，模拟「切后端时面板恰好还没关」
    root.sidebar.video_export_visible = true;
    root.sidebar.route = Route::VideoExport;

    root.update(Message::VideoExport(VideoExportAction::ClosePanel));

    assert!(!root.sidebar.video_export_visible);
    assert_ne!(
        root.sidebar.route,
        Route::Arrangement,
        "CPU 下关闭视频导出不得落进工程走带"
    );
    assert!(!root.is_arrangement_mode());
}

/// GPU 回归：关闭视频导出仍回到工程走带（与音频面板同口径）。
#[test]
fn test_gpu_video_export_close_keeps_arrangement() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);
    root.sidebar.video_export_visible = true;
    root.sidebar.route = Route::VideoExport;

    root.update(Message::VideoExport(VideoExportAction::ClosePanel));

    assert!(!root.sidebar.video_export_visible);
    assert_eq!(root.sidebar.route, Route::Arrangement);
}

/// CPU 下即使走带分组被外部置位，出口仍要把它清掉（不变量：CPU 分支不落走带）。
#[test]
fn test_cpu_rendering_panel_exit_clears_stale_project_group() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    root.sidebar.active_group = Some(GroupId::Project);

    root.exit_rendering_panel();

    assert_eq!(
        root.sidebar.active_group,
        Some(GroupId::PianoRoll),
        "走带分组残留必须被清回钢琴卷帘组"
    );
    assert_ne!(root.sidebar.route, Route::Arrangement);
}

/// 出口守卫的落点必须真的可用：CPU 下关闭面板后卷帘可见、走带语义为空。
#[test]
fn test_cpu_exit_route_is_piano_roll_visible() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    root.sidebar.piano_roll_visible = false;
    root.sidebar.route = Route::AudioExport;

    root.exit_rendering_panel();

    assert!(
        root.sidebar.piano_roll_visible,
        "关闭渲染面板后必须回到可见的钢琴卷帘（否则用户面对空白区）"
    );
    assert_eq!(root.sidebar.route, Route::File, "默认落点为文件子面板");
}
