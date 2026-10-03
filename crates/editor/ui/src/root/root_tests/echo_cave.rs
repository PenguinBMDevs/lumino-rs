//! 关于页回声洞彩蛋（UI-006）—— 「消息 → 处理器 → 状态 → 视图」接线测试
//!
//! 相位时间轴与边界口径的**确定性**验证在 `lumino-ui-core` 的
//! `state::echo_cave::tests`（注入 `Instant`，无 `sleep`）。本文件只覆盖真实接线：
//! 菜单切到关于页能装填打字机、点击消息落到状态、`AnimationTick` 逐帧推进并最终收敛、
//! 三种相位下视图均可构建。
//!
//! 注：`handle_animation_tick` 内部取真实时钟，故推进类断言用「有界等待」而非固定帧数。

use crate::Message;
use crate::root::Root;
use crate::root::handlers::{MessageRouter, SettingsHandler};
use crate::settings::Event as SettingsEvent;
use lumino_core::storage::config::UiConfig;
use lumino_ui_core::state::echo_cave::{ECHO_TEXTS, EchoPhase};
use std::time::{Duration, Instant};

/// 构造「设置对话框 Root + 只挂 SettingsHandler 的路由器」
fn fixture() -> (Root, MessageRouter) {
    let root = Root::new_settings_dialog("Dark", &UiConfig::default());
    let mut router = MessageRouter::new();
    router.register(Box::new(SettingsHandler::new()));
    (root, router)
}

/// 模拟一次彩蛋文本点击（走真实消息路由器）
fn click_echo(root: &mut Root, router: &mut MessageRouter) {
    router.route(root, Message::Settings(SettingsEvent::EchoCaveClicked));
}

/// 模拟切换到关于页（走真实消息路由器）
fn select_about(root: &mut Root, router: &mut MessageRouter) {
    router.route(
        root,
        Message::Settings(SettingsEvent::MenuSelected(
            crate::settings::MENU_INDEX_ABOUT,
        )),
    );
}

/// 有界推进：逐帧喂 `AnimationTick` 直到不再动画（或超过 `max_frames`）
fn drive_until_settled(root: &mut Root, max_frames: usize) -> usize {
    for frame in 0..max_frames {
        if !root.state.echo_cave.is_animating() {
            return frame;
        }
        assert!(root.try_handle_direct(&Message::AnimationTick));
        std::thread::sleep(Duration::from_millis(4));
    }
    max_frames
}

#[test]
fn test_new_dialog_state_is_idle_and_does_not_ask_for_frames() {
    // 关键护栏：设置面板刚打开（关于页尚未进入）时不得要求逐帧驱动，
    // 否则主窗口 / 对话框会在无动画期间空转重绘
    let (root, _router) = fixture();
    assert_eq!(root.state.echo_cave.phase(), EchoPhase::Idle);
    assert!(!root.state.echo_cave.is_animating());
    assert_eq!(root.state.echo_cave.text(), "");
    {
        let _ = root.view();
    }
}

#[test]
fn test_select_about_starts_typewriter() {
    let (mut root, mut router) = fixture();
    select_about(&mut root, &mut router);

    assert_eq!(
        root.state.echo_cave.phase(),
        EchoPhase::Typing,
        "切到关于页应装填打字机（唯一装填点）"
    );
    assert!(root.state.echo_cave.is_animating());
    assert_eq!(root.state.echo_cave.index(), 0);
    {
        let _ = root.view();
    }
}

#[test]
fn test_other_menu_selection_does_not_start_typewriter() {
    // 防回归：只有关于页装填；切到别的页面不得启动动画
    let (mut root, mut router) = fixture();
    router.route(&mut root, Message::Settings(SettingsEvent::MenuSelected(0)));
    assert_eq!(root.state.echo_cave.phase(), EchoPhase::Idle);
    assert!(!root.state.echo_cave.is_animating());
}

#[test]
fn test_animation_tick_advances_and_terminates() {
    let (mut root, mut router) = fixture();
    select_about(&mut root, &mut router);

    // 首帧 dt≈0（起点相位时间为装填时刻），随后应真的逐字揭示。
    // 20cps 下第一个字符需 50ms，故用有界帧数推进而非固定 sleep。
    assert!(root.try_handle_direct(&Message::AnimationTick));
    let mut advanced = false;
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(8));
        assert!(root.try_handle_direct(&Message::AnimationTick));
        if !root.state.echo_cave.text().is_empty() {
            advanced = true;
            break;
        }
    }
    assert!(advanced, "AnimationTick 应推进打字机（接线正确性）");

    let frames = drive_until_settled(&mut root, 800);
    assert!(
        frames < 800,
        "打字机必须在有界帧数内收敛，不得长期占用逐帧重绘"
    );
    assert!(!root.state.echo_cave.is_animating());
    assert_eq!(root.state.echo_cave.text(), ECHO_TEXTS[0]);
    {
        let _ = root.view();
    }
}

#[test]
fn test_click_enters_blink_and_cycles_to_next_text() {
    let (mut root, mut router) = fixture();
    select_about(&mut root, &mut router);

    // 先把打字机推到收敛（有界等待，防慢机误判）
    let frames = drive_until_settled(&mut root, 800);
    assert!(
        !root.state.echo_cave.is_animating(),
        "打字机应在有界帧数内收敛，实际 {frames} 帧"
    );
    assert_eq!(root.state.echo_cave.phase(), EchoPhase::Idle);

    click_echo(&mut root, &mut router);
    assert_eq!(
        root.state.echo_cave.phase(),
        EchoPhase::Blinking,
        "点击应进入闪烁退出"
    );
    // 闪烁期视图可构建（alpha 分支）
    {
        let _ = root.view();
    }

    // 推进到闪烁结束：应切到「下一条的打字机」，而不是直接回到 Idle。
    // 上限 200 帧（4ms/帧 ≈ 0.8s）覆盖 0.5s 闪烁时长。
    let mut frames = 0;
    while root.state.echo_cave.phase() == EchoPhase::Blinking && frames < 200 {
        assert!(root.try_handle_direct(&Message::AnimationTick));
        std::thread::sleep(Duration::from_millis(4));
        frames += 1;
    }
    assert!(frames < 200, "闪烁相位必须在有界帧数内退出");
    assert_eq!(
        root.state.echo_cave.phase(),
        EchoPhase::Typing,
        "闪烁后应进入下一条的打字机"
    );
    assert_eq!(root.state.echo_cave.index(), 1, "索引应 +1");

    // 再推到收敛，确认整轮循环可完整走完
    let frames = drive_until_settled(&mut root, 800);
    assert!(frames < 800, "下一条打字机应在有界帧数内收敛");
    assert_eq!(
        root.state.echo_cave.text(),
        ECHO_TEXTS[1],
        "循环后应显示第 2 条完整文案"
    );
}

#[test]
fn test_rapid_clicks_do_not_stall_the_cycle() {
    // 验收 4 的接线级护栏：闪烁期狂点不得把状态钉死
    let (mut root, mut router) = fixture();
    select_about(&mut root, &mut router);

    for _ in 0..60 {
        click_echo(&mut root, &mut router);
        assert!(root.try_handle_direct(&Message::AnimationTick));
        std::thread::sleep(Duration::from_millis(2));
        let phase = root.state.echo_cave.phase();
        if phase != EchoPhase::Blinking && phase != EchoPhase::Typing {
            break;
        }
    }

    // 无论点了多少次，最终必须收敛到 Idle 且索引合法
    let deadline = Instant::now() + Duration::from_secs(8);
    while root.state.echo_cave.is_animating() && Instant::now() < deadline {
        assert!(root.try_handle_direct(&Message::AnimationTick));
        std::thread::sleep(Duration::from_millis(8));
    }
    assert!(!root.state.echo_cave.is_animating(), "连点后必须能收敛");
    assert!(
        root.state.echo_cave.index() < ECHO_TEXTS.len(),
        "索引必须始终在合法范围内，实际 {}",
        root.state.echo_cave.index()
    );
    {
        let _ = root.view();
    }
}

#[test]
fn test_echo_view_builds_in_every_phase() {
    let (mut root, mut router) = fixture();
    // Idle（未装填）
    {
        let _ = root.view();
    }
    // Typing
    select_about(&mut root, &mut router);
    std::thread::sleep(Duration::from_millis(20));
    assert!(root.try_handle_direct(&Message::AnimationTick));
    {
        let _ = root.view();
    }
    // Blinking
    click_echo(&mut root, &mut router);
    {
        let _ = root.view();
    }
}
