//! `Root::force_horizontal_roll` 单元测试
//!
//! 该方法是「切到 CPU 洋葱皮时自动退出纵向卷帘」的唯一入口，
//! 语义必须与 `handle_sidebar_event` 的退出纵向分支一致：
//! `is_vertical_roll` / `sidebar.roll_bar_active` / `right_sidebar.roll_mode`
//! 三者同步，且横向视图备份被恢复。

use crate::root::Root;
use crate::sidebar::RollBarButton;
use lumino_core::storage::config::UiConfig;

/// 横向视图的基准缩放（备份值，用于验证 restore 真的生效）。
const HORIZONTAL_ZOOM_X: f32 = 0.5;
const HORIZONTAL_ZOOM_Y: f32 = 2.0;
/// 纵向卷帘时的缩放（被 `fit_vertical_keyboard_to_viewport` 改写后的值）。
const VERTICAL_ZOOM_Y: f32 = 6.0;

/// 构造一个「刚点击了纵向按钮」的 Root（进入分支同口径）。
fn root_with_vertical_roll() -> Root {
    let mut root = Root::new(&UiConfig::default());
    root.editor.editor_state.view.zoom_x = HORIZONTAL_ZOOM_X;
    root.editor.editor_state.view.zoom_y = HORIZONTAL_ZOOM_Y;
    root.editor.editor_state.save_horizontal_backup();

    root.sidebar.roll_bar_active = Some(RollBarButton::Vertical);
    root.right_sidebar.roll_mode = RollBarButton::Vertical;
    root.editor.editor_state.is_vertical_roll = true;
    root.editor.editor_state.view.zoom_y = VERTICAL_ZOOM_Y;
    root
}

#[test]
fn test_force_horizontal_roll_restores_horizontal_state() {
    let mut root = root_with_vertical_roll();

    assert!(
        root.force_horizontal_roll(),
        "纵向卷帘调用应真的发生降级（返回 true）"
    );

    assert!(
        !root.editor.editor_state.is_vertical_roll,
        "is_vertical_roll 必须复位"
    );
    assert_eq!(
        root.sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "侧边栏必须点亮横向按钮"
    );
    assert_eq!(
        root.right_sidebar.roll_mode,
        RollBarButton::Horizontal,
        "右侧栏 roll_mode 必须与侧边栏一致（瀑布流预览入口显隐依赖它）"
    );
    assert!(
        (root.editor.editor_state.view.zoom_y - HORIZONTAL_ZOOM_Y).abs() < f32::EPSILON,
        "必须恢复横向视图备份（当前 zoom_y = {}）",
        root.editor.editor_state.view.zoom_y
    );
    assert!(
        (root.editor.editor_state.view.zoom_x - HORIZONTAL_ZOOM_X).abs() < f32::EPSILON,
        "zoom_x 同样取自备份"
    );
}

#[test]
fn test_force_horizontal_roll_noop_when_already_horizontal() {
    let mut root = Root::new(&UiConfig::default());
    // 用户手动熄灭两个卷帘按钮后的状态：None 也必须被原样保留
    root.sidebar.roll_bar_active = None;
    root.right_sidebar.roll_mode = RollBarButton::Vertical;
    root.editor.editor_state.view.zoom_y = HORIZONTAL_ZOOM_Y;

    assert!(!root.force_horizontal_roll(), "已是横向：直接返回 false");

    assert_eq!(
        root.sidebar.roll_bar_active, None,
        "非纵向时不得改动卷帘按钮状态（无副作用）"
    );
    assert_eq!(
        root.right_sidebar.roll_mode,
        RollBarButton::Vertical,
        "非纵向时不得改动右侧栏 roll_mode（无副作用）"
    );
    assert!(!root.editor.editor_state.is_vertical_roll);
    assert!(
        (root.editor.editor_state.view.zoom_y - HORIZONTAL_ZOOM_Y).abs() < f32::EPSILON,
        "非纵向时不得改动视图缩放"
    );
}

#[test]
fn test_force_horizontal_roll_is_idempotent() {
    let mut root = root_with_vertical_roll();
    assert!(root.force_horizontal_roll());
    assert!(
        !root.force_horizontal_roll(),
        "第二次调用应短路（幂等），避免重复 restore/失效缓存"
    );
    assert_eq!(
        root.sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal)
    );
    assert!(!root.editor.editor_state.is_vertical_roll);
}
