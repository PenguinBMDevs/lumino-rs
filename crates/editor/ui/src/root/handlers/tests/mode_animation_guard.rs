//! 瀑布流切换**过渡态**竞态测试（Step 4b）
//!
//! 状态机在动画 tick 里改 `AppMode`，不是点击的那一刻：点标题栏进瀑布流后，
//! `current_mode` 仍是 `Editor`，直到动画收敛（`position >= 0.5` 且未活跃）那一帧
//! 才翻成 `Waterfall`。因此入口闸/切换闸必须看**过渡态**
//! （`ToggleAnimationState::is_heading_to_waterfall`），只盯 `current_mode` 会漏掉：
//!
//! 点进瀑布流（动画进行中）→ 此刻切到 CPU 后端 → 动画收敛 → 会话被带进瀑布流。
//!
//! 本模块锁定 `tick_mode_animation` 的纵深防御：CPU 后端下任何朝向瀑布流的过渡
//! 都会被取消；GPU 路径动画行为零变化。

use crate::message::Message;
use crate::sidebar::Route;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::OnionSkinBackend;

use super::{assert_warning_toast, root_with_backend};

/// 场景复现：点标题栏进瀑布流（动画已启动，`position` 仍为 0，`current_mode` 仍是
/// Editor）→ 此刻后端已是 CPU → 下一帧 tick **不得**把 `AppMode` 翻成 Waterfall。
#[test]
fn test_cpu_cancels_in_flight_waterfall_transition() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    // 动画刚启动：目标指向瀑布流，进度尚未移动（竞态窗口）
    root.state.toggle_animation.animate_to(1.0);
    assert_eq!(root.state.toggle_animation.position, 0.0);
    assert_eq!(root.state.current_mode, AppMode::Editor);

    root.update(Message::AnimationTick);

    assert_eq!(
        root.state.current_mode,
        AppMode::Editor,
        "CPU 后端下动画收敛也不得把 AppMode 翻成瀑布流"
    );
    assert_eq!(
        root.state.toggle_animation.target, 0.0,
        "过渡必须被取消（目标归零）"
    );
    assert_eq!(root.state.toggle_animation.position, 0.0);
    assert!(!root.state.toggle_animation.active);
    assert_warning_toast(&root, "CPU 洋葱皮后端不支持全屏瀑布流");
}

/// 同一竞态的第二形态：动画已收敛（position=1.0、active=false），但 `AppMode`
/// 尚未同步的那一帧被切到 CPU —— tick 仍必须先取消，不得翻成 Waterfall。
#[test]
fn test_cpu_cancels_converged_but_unsynced_waterfall_transition() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    root.state.toggle_animation.target = 1.0;
    root.state.toggle_animation.position = 1.0;
    root.state.toggle_animation.velocity = 0.0;
    root.state.toggle_animation.active = false;

    root.update(Message::AnimationTick);

    assert_eq!(
        root.state.current_mode,
        AppMode::Editor,
        "已收敛但 mode 未同步的窗口同样不得被 CPU 带进瀑布流"
    );
    assert!(!root.state.toggle_animation.is_heading_to_waterfall());
}

/// CPU 下反复 tick 也不会「积累」出瀑布流（取消是终态，不是每帧抖动）。
#[test]
fn test_cpu_transition_stays_cancelled_across_ticks() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    root.state.toggle_animation.animate_to(1.0);

    for _ in 0..8 {
        root.update(Message::AnimationTick);
        assert_eq!(root.state.current_mode, AppMode::Editor);
    }
    assert!(!root.state.toggle_animation.is_heading_to_waterfall());
}

/// CPU 下取消过渡不得顺带改路由（只归零动画与 AppMode）。
#[test]
fn test_cpu_transition_cancel_leaves_route_untouched() {
    let mut root = root_with_backend(OnionSkinBackend::Cpu);
    let route_before = root.sidebar.route;
    root.state.toggle_animation.animate_to(1.0);

    root.update(Message::AnimationTick);

    assert_eq!(root.sidebar.route, route_before);
    assert_ne!(root.sidebar.route, Route::Arrangement);
}

/// GPU 回归：同样的动画状态在 GPU 后端正常收敛到瀑布流（纵深防御不影响默认路径）。
#[test]
fn test_gpu_finishes_waterfall_transition() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);
    root.state.toggle_animation.target = 1.0;
    root.state.toggle_animation.position = 1.0;
    root.state.toggle_animation.velocity = 0.0;
    root.state.toggle_animation.active = false;

    root.update(Message::AnimationTick);

    assert_eq!(
        root.state.current_mode,
        AppMode::Waterfall,
        "GPU 下动画照常完成到瀑布流"
    );
    assert!(root.toast.is_empty(), "GPU 路径不得出现 CPU 拦截 Toast");
}

/// GPU 回归：动画进行中（未收敛）不得被提前翻转模式。
#[test]
fn test_gpu_animation_in_flight_stays_editor_until_converged() {
    let mut root = root_with_backend(OnionSkinBackend::Gpu);
    root.state.toggle_animation.animate_to(1.0);

    root.update(Message::AnimationTick);

    assert_eq!(
        root.state.current_mode,
        AppMode::Editor,
        "动画未收敛时 AppMode 仍为 Editor（与旧行为一致）"
    );
    assert!(
        root.state.toggle_animation.is_heading_to_waterfall(),
        "GPU 下过渡不得被取消"
    );
}
