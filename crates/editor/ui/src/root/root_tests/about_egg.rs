//! 关于页 logo 彩蛋（UI-007）—— 「消息 → 处理器 → 状态 → 视图」接线测试
//!
//! 相位时间轴与几何口径的**确定性**验证在 `lumino-ui-core` 的
//! `state::about_egg::tests`（注入 `Instant`，无 `sleep`）。本文件只覆盖真实接线：
//! 点击消息经路由器落到彩蛋状态、满阈值进入飞行、`AnimationTick` 逐帧推进并最终
//! 收尾、两种状态（原位 / 飞行 / 已消失）下视图均可构建。
//!
//! 注：`handle_animation_tick` 内部取真实时钟，故推进类断言用「有界等待」而非固定
//! 帧数；序列硬超时（4s）保证了一定的终止性。

use crate::Message;
use crate::root::Root;
use crate::root::handlers::{MessageRouter, SettingsHandler};
use crate::settings::Event as SettingsEvent;
use lumino_core::storage::config::UiConfig;
use lumino_ui_core::state::about_egg::{EggPhase, logo_has_vanished, reset_logo_vanished};
use std::time::{Duration, Instant};

/// 进程级「已消失」标志是共享状态：串行化本文件的用例，避免并行互污染
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 构造「设置对话框 Root + 只挂 SettingsHandler 的路由器」
fn fixture() -> (Root, MessageRouter) {
    let mut root = Root::new_settings_dialog("Dark", &UiConfig::default());
    // 生产中视口与点击点由 Host 注入（`set_viewport` 每帧、`set_click_point` 在路由前）
    root.state.about_egg.set_viewport(720.0, 540.0);
    root.state.about_egg.set_click_point(Some((305.0, 121.0)));

    let mut router = MessageRouter::new();
    router.register(Box::new(SettingsHandler::new()));
    (root, router)
}

/// 模拟一次 logo 点击（走真实消息路由器）
fn click_logo(root: &mut Root, router: &mut MessageRouter) {
    router.route(root, Message::Settings(SettingsEvent::AboutLogoClicked));
}

#[test]
fn test_fifteen_clicks_trigger_airborne_sequence() {
    let _guard = lock();
    reset_logo_vanished();
    let (mut root, mut router) = fixture();

    for _ in 0..14 {
        click_logo(&mut root, &mut router);
    }
    assert_eq!(root.state.about_egg.clicks(), 14, "未满阈值应逐次累计");
    assert!(!root.state.about_egg.is_airborne(), "未满阈值不得脱离原位");
    assert!(
        root.state.about_egg.is_visible_in_page(),
        "未触发彩蛋时 logo 仍在原位渲染"
    );
    // 原位（晃动）路径视图可构建
    {
        let _ = root.view();
    }

    click_logo(&mut root, &mut router);
    assert!(
        root.state.about_egg.is_airborne(),
        "满 15 连点应触发彩蛋序列"
    );
    assert_eq!(root.state.about_egg.phase(), EggPhase::Spin);
    assert!(
        !root.state.about_egg.is_visible_in_page(),
        "飞行期原位不得再渲染 logo（否则重影）"
    );
    // 飞行路径视图可构建（含最顶层悬浮层 Stack 组装）
    {
        let _ = root.view();
    }
}

#[test]
fn test_animation_tick_advances_and_sequence_terminates() {
    let _guard = lock();
    reset_logo_vanished();
    let (mut root, mut router) = fixture();
    for _ in 0..15 {
        click_logo(&mut root, &mut router);
    }
    assert!(root.state.about_egg.is_airborne());

    // 首帧 dt≈0（起点相位时间为点击时刻），隔一帧后转动角必须真的增长
    assert!(root.try_handle_direct(&Message::AnimationTick));
    std::thread::sleep(Duration::from_millis(16));
    assert!(root.try_handle_direct(&Message::AnimationTick));
    assert!(
        root.state.about_egg.spin_angle() > 0.0,
        "AnimationTick 应推进彩蛋转动角（接线正确性）"
    );

    // 有界等待收尾：硬超时 4s 保证终止，这里给 8s 上限防慢机误判
    let deadline = Instant::now() + Duration::from_secs(8);
    while root.state.about_egg.is_animating() && Instant::now() < deadline {
        assert!(root.try_handle_direct(&Message::AnimationTick));
        std::thread::sleep(Duration::from_millis(8));
    }

    assert!(
        !root.state.about_egg.is_animating(),
        "彩蛋序列必须在硬超时内收尾，不得长期占用逐帧重绘"
    );
    assert_eq!(root.state.about_egg.phase(), EggPhase::Vanished);
    assert!(logo_has_vanished(), "收尾应写入进程级「已消失」标志");
    // 消失后视图仍可构建（无 logo、无悬浮层）
    {
        let _ = root.view();
    }

    // 模拟「关闭设置面板 → 重新打开」：全新 Root 应直接处于已消失
    let reopened = Root::new_settings_dialog("Dark", &UiConfig::default());
    assert!(
        !reopened.state.about_egg.is_visible_in_page(),
        "重开设置面板不应再出现 logo（进程内保持）"
    );

    reset_logo_vanished();
}
