//! CPU 洋葱皮后端**入口**守卫测试（Step 4b）
//!
//! 断言口径：每个不兼容入口「状态零变化 + Warning Toast」——拦截必须发生在状态写入
//! 之前，否则会出现「按钮已点亮 / 路由已切一半」的自相矛盾状态；GPU 后端同样的入口
//! 行为与旧版完全一致（默认路径零回归）。
//!
//! 相邻模块（同一 Step 的另两类守卫，拆分只为单文件 < 400 行）：
//! - `rendering_panel_exit`：关闭渲染面板的**出口**不得落进被禁用的走带；
//! - `mode_animation_guard`：瀑布流切换**过渡态**竞态（动画进行中切 CPU）。

use crate::message::{Message, VideoExportAction};
use crate::right_sidebar::RightSidebarPanel;
use crate::sidebar::{self, GroupId, RollBarButton, Route};
use crate::state::root_state::VideoExportOverlayState;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::OnionSkinBackend;
use lumino_message::RightSidebarAction;

use super::{assert_warning_toast, root_with_backend};

// ─── 判定源 ───────────────────────────────────────────────

#[test]
fn test_is_cpu_onion_backend_follows_settings() {
    assert!(!root_with_backend(OnionSkinBackend::Gpu).is_cpu_onion_backend());
    assert!(root_with_backend(OnionSkinBackend::Cpu).is_cpu_onion_backend());
}

// ─── CPU：纵向卷帘 ─────────────────────────────────────────

#[test]
fn test_cpu_blocks_vertical_roll_button() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    assert_eq!(
        root.sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal)
    );

    root.handle_sidebar_event(sidebar::Event::RollBarToggled(RollBarButton::Vertical));

    assert!(
        !root.sidebar.is_vertical_roll(),
        "CPU 后端下纵向按钮不得生效（否则 CPU 叠加层坐标错位）"
    );
    assert_eq!(
        root.sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "按钮点亮状态必须原样保留"
    );
    assert_eq!(
        root.right_sidebar.roll_mode,
        RollBarButton::Horizontal,
        "右侧栏 roll_mode 不得被改（瀑布流预览入口显隐依赖它）"
    );
    assert!(!root.editor.editor_state.is_vertical_roll);
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持纵向卷帘");
}

// ─── CPU：工程走带 / 全屏瀑布流 ────────────────────────────

#[test]
fn test_cpu_blocks_project_group() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.handle_sidebar_event(sidebar::Event::GroupToggled(GroupId::Project));

    assert_eq!(
        root.sidebar.active_group,
        Some(GroupId::PianoRoll),
        "分组不得切换"
    );
    assert_eq!(root.sidebar.route, Route::File, "路由不得切到工程走带");
    assert!(root.sidebar.piano_roll_visible, "钢琴卷帘不得被隐藏");
    assert_eq!(root.state.current_mode, AppMode::Editor);
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持工程走带视图");
}

#[test]
fn test_cpu_blocks_waterfall_group() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.handle_sidebar_event(sidebar::Event::GroupToggled(GroupId::Waterfall));

    assert_eq!(
        root.sidebar.active_group,
        Some(GroupId::PianoRoll),
        "分组不得切换"
    );
    assert_eq!(
        root.state.current_mode,
        AppMode::Editor,
        "AppMode 必须保持 Editor"
    );
    assert!(!root.state.toggle_animation.active, "切换动画不得启动");
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持全屏瀑布流");
}

// ─── CPU：走带 / 视频导出的路由与面板事件 ──────────────────

#[test]
fn test_cpu_blocks_arrangement_and_video_export_routes() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.handle_sidebar_event(sidebar::Event::RouteUpdated(Route::Arrangement));
    assert_eq!(root.sidebar.route, Route::File, "走带路由必须被拦截");
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持工程走带视图");

    // 防御性兜底：`PanelToggled(Arrangement)` 同样不得放行
    root.handle_sidebar_event(sidebar::Event::PanelToggled(Route::Arrangement));
    assert_eq!(root.sidebar.route, Route::File);
    assert!(!root.sidebar.panel_visible || root.sidebar.panel_route != Route::Arrangement);
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持工程走带视图");

    root.handle_sidebar_event(sidebar::Event::RouteUpdated(Route::VideoExport));
    assert!(
        !root.sidebar.video_export_visible,
        "视频导出面板不得打开（只打开不导出同样不可用）"
    );
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持视频导出");

    root.handle_sidebar_event(sidebar::Event::PanelToggled(Route::VideoExport));
    assert!(!root.sidebar.video_export_visible);
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持视频导出");
}

/// 音频导出与 CPU 后端无冲突：不能被入口闸误伤（回归）。
#[test]
fn test_cpu_allows_audio_export_route() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.handle_sidebar_event(sidebar::Event::RouteUpdated(Route::AudioExport));

    assert!(
        root.sidebar.audio_export_visible,
        "音频导出不受 CPU 后端影响"
    );
    assert!(root.toast.is_empty(), "合法入口不得产生任何 Toast");
}

// ─── CPU：标题栏模式切换 ───────────────────────────────────

#[test]
fn test_cpu_blocks_mode_toggle_into_waterfall() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.update(Message::ModeToggled);

    assert_eq!(
        root.state.current_mode,
        AppMode::Editor,
        "CPU 后端下不得进入全屏瀑布流"
    );
    assert!(!root.state.toggle_animation.active, "切换动画不得启动");
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持全屏瀑布流");
}

// ─── CPU：右侧栏钢琴瀑布流预览 ─────────────────────────────

#[test]
fn test_cpu_blocks_piano_waterfall_panel() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);

    assert!(!root.right_sidebar.panel_visible, "面板不得打开");
    assert_eq!(
        root.right_sidebar.active_panel,
        RightSidebarPanel::ImageToMidi,
        "面板归属不得切换"
    );
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持钢琴瀑布流预览");
}

/// 万一面板已是打开态（历史状态残留），「收起」必须放行——否则用户关不掉。
#[test]
fn test_cpu_allows_closing_already_open_piano_waterfall_panel() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    root.right_sidebar
        .switch_panel(RightSidebarPanel::PianoWaterfall);

    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);

    assert!(!root.right_sidebar.panel_visible, "打开态下点击应能收起");
    assert!(root.toast.is_empty(), "收起是合法操作，不应弹拦截 Toast");
}

// ─── CPU：视频导出面板动作 ─────────────────────────────────

#[test]
fn test_cpu_blocks_video_export_panel_actions() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.update(Message::VideoExport(VideoExportAction::OpenPanel));
    assert!(
        !root.sidebar.video_export_visible,
        "OpenPanel 不得置面板可见"
    );
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持视频导出");

    // StartExport 必须早于 `overlay = Exporting` 与事件发射（CPU 下导出必然失败）
    root.update(Message::VideoExport(VideoExportAction::StartExport));
    assert!(
        !matches!(
            root.state.video_export_dialog.overlay,
            VideoExportOverlayState::Exporting
        ),
        "StartExport 不得进入导出中状态"
    );
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持视频导出");
}

/// 关闭/取消仍然放行：收尾路径不能被闸门堵住。
#[test]
fn test_cpu_allows_video_export_close_and_cancel() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);

    root.update(Message::VideoExport(VideoExportAction::ClosePanel));
    assert!(!root.sidebar.video_export_visible);
    root.update(Message::VideoExport(VideoExportAction::DismissOverlay));
    assert!(root.toast.is_empty(), "收尾动作不得弹拦截 Toast");
}

// ─── GPU：旧行为回归（默认后端零变化）──────────────────────

#[test]
fn test_gpu_keeps_vertical_roll_behavior() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.handle_sidebar_event(sidebar::Event::RollBarToggled(RollBarButton::Vertical));

    assert!(root.sidebar.is_vertical_roll(), "GPU 下纵向卷帘照常生效");
    assert_eq!(root.right_sidebar.roll_mode, RollBarButton::Vertical);
    assert!(root.toast.is_empty(), "GPU 下不得出现 CPU 拦截 Toast");
}

#[test]
fn test_gpu_keeps_project_group_behavior() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.handle_sidebar_event(sidebar::Event::GroupToggled(GroupId::Project));

    assert_eq!(root.sidebar.active_group, Some(GroupId::Project));
    assert_eq!(root.sidebar.route, Route::Arrangement);
    assert!(!root.sidebar.piano_roll_visible);
    assert!(root.toast.is_empty());
}

#[test]
fn test_gpu_keeps_mode_toggle_behavior() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.update(Message::ModeToggled);

    assert_eq!(root.state.current_mode, AppMode::Waterfall);
    assert!(root.toast.is_empty());
}

#[test]
fn test_gpu_keeps_video_export_panel_behavior() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.update(Message::VideoExport(VideoExportAction::OpenPanel));
    assert!(root.sidebar.video_export_visible, "GPU 下面板照常打开");
    assert_eq!(root.sidebar.route, Route::VideoExport);
    assert!(root.toast.is_empty());

    root.update(Message::VideoExport(VideoExportAction::ClosePanel));
    assert!(!root.sidebar.video_export_visible);
}

#[test]
fn test_gpu_keeps_piano_waterfall_panel_behavior() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);

    assert!(root.right_sidebar.panel_visible, "GPU 下预览面板照常打开");
    assert_eq!(
        root.right_sidebar.active_panel,
        RightSidebarPanel::PianoWaterfall
    );
    assert!(root.toast.is_empty());
}

// ─── 切换后清退预览面板（Step 7）────────────────────────────

#[test]
fn test_close_piano_waterfall_panel_only_when_active() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);

    assert!(
        !root.close_piano_waterfall_panel(),
        "面板未激活时返回 false（调用方据此决定不弹 Toast）"
    );

    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);
    assert!(root.right_sidebar.panel_visible);

    assert!(root.close_piano_waterfall_panel(), "激活时应真的关闭");
    assert!(!root.right_sidebar.panel_visible);
    assert_eq!(
        root.right_sidebar.active_panel,
        RightSidebarPanel::PianoWaterfall,
        "只收起可见性，保留面板归属（切回 GPU 后原位重开）"
    );
    assert!(
        !root.close_piano_waterfall_panel(),
        "幂等：已关闭时返回 false"
    );
}
