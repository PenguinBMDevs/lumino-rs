//! 侧边栏单测 —— 卷帘面板底部按钮（横向/纵向三条杠）
//!
//! 覆盖：默认熄灭、点击点亮再点击熄灭、两按钮互斥、仅在卷帘面板显示。

use crate::sidebar::*;

/// 卷帘底部按钮默认均未点亮
#[test]
fn test_roll_bar_buttons_inactive_by_default() {
    let sidebar = Sidebar::new();
    assert!(sidebar.roll_bar_active.is_none(), "默认两个按钮均应熄灭");
}

/// 点击卷帘底部按钮点亮，再次点击同一按钮熄灭
#[test]
fn test_roll_bar_button_toggles_off_on_second_click() {
    let mut sidebar = Sidebar::new();

    let redraw = sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert!(redraw, "亮灯状态变化应触发重绘");
    assert_eq!(sidebar.roll_bar_active, Some(RollBarButton::Horizontal));

    let redraw = sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert!(redraw, "熄灯状态变化应触发重绘");
    assert!(sidebar.roll_bar_active.is_none(), "再次点击应关闭该按钮");
}

/// 两个卷帘底部按钮的打开状态互斥
#[test]
fn test_roll_bar_buttons_are_mutually_exclusive() {
    let mut sidebar = Sidebar::new();

    sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    sidebar.update(Event::RollBarToggled(RollBarButton::Vertical));
    assert_eq!(
        sidebar.roll_bar_active,
        Some(RollBarButton::Vertical),
        "打开纵向按钮时横向按钮应熄灭"
    );

    sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert_eq!(
        sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "打开横向按钮时纵向按钮应熄灭"
    );
}

/// 卷帘底部按钮仅在处于卷帘面板时显示
#[test]
fn test_roll_bar_visible_only_in_piano_roll_panel() {
    let mut sidebar = Sidebar::new();
    assert!(sidebar.is_piano_roll_panel(), "默认处于卷帘面板");

    // 工程走带：卷帘隐藏 → 按钮不显示
    sidebar.update(Event::GroupToggled(GroupId::Project));
    assert!(!sidebar.is_piano_roll_panel(), "工程走带界面不应显示");

    // 回到卷帘组 → 按钮恢复显示
    sidebar.update(Event::GroupToggled(GroupId::PianoRoll));
    assert!(sidebar.is_piano_roll_panel(), "返回卷帘面板后应显示");

    // 瀑布流：卷帘隐藏 → 按钮不显示
    sidebar.update(Event::GroupToggled(GroupId::Waterfall));
    assert!(!sidebar.is_piano_roll_panel(), "瀑布流界面不应显示");

    // 音频导出面板占据卷帘区域 → 按钮不显示
    sidebar.update(Event::GroupToggled(GroupId::PianoRoll));
    sidebar.update(Event::RouteUpdated(Route::AudioExport));
    assert!(!sidebar.is_piano_roll_panel(), "音频导出面板不应显示");
}
