//! 侧边栏单测 —— 卷帘面板底部按钮（横向/纵向卷帘）
//!
//! 覆盖：默认横向、点击点亮再点击熄灭、两按钮互斥、仅在卷帘面板显示、纵向判定。

use crate::sidebar::*;

/// 卷帘底部按钮默认进入横向卷帘（横向点亮）
#[test]
fn test_roll_bar_default_is_horizontal() {
    let sidebar = Sidebar::new();
    assert_eq!(
        sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "默认应处于横向卷帘"
    );
    assert!(!sidebar.is_vertical_roll(), "默认不处于纵向卷帘");
}

/// 判定是否处于纵向卷帘
#[test]
fn test_is_vertical_roll_reflects_active() {
    let mut sidebar = Sidebar::new();
    assert!(!sidebar.is_vertical_roll());

    sidebar.update(Event::RollBarToggled(RollBarButton::Vertical));
    assert!(sidebar.is_vertical_roll(), "切换纵向后应判定为纵向卷帘");

    sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert!(!sidebar.is_vertical_roll(), "切回横向后不应为纵向卷帘");
}

/// 点击已点亮的卷帘按钮将其熄灭，再次点击重新点亮
///
/// 默认进入横向卷帘（Horizontal 点亮），故首次点击先熄灭，再次点击恢复。
#[test]
fn test_roll_bar_button_toggles_off_on_second_click() {
    let mut sidebar = Sidebar::new();
    assert_eq!(
        sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "默认应处于横向卷帘（点亮）"
    );

    let redraw = sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert!(redraw, "熄灯状态变化应触发重绘");
    assert!(sidebar.roll_bar_active.is_none(), "点击已点亮按钮应熄灭");

    let redraw = sidebar.update(Event::RollBarToggled(RollBarButton::Horizontal));
    assert!(redraw, "亮灯状态变化应触发重绘");
    assert_eq!(
        sidebar.roll_bar_active,
        Some(RollBarButton::Horizontal),
        "再次点击应重新点亮"
    );
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
