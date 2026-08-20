//! 右侧栏跟随钢琴卷帘 UI 显隐测试
//!
//! 修复背景：离开钢琴卷帘（进入工程走带）后，钢琴卷帘 UI 隐藏，
//! 但右侧栏仍渲染在走带视图右侧（fb2abc93 将右侧栏移入钢琴卷帘
//! 编辑区时遗漏了走带分支）。修复后右侧栏仅在钢琴卷帘编辑区渲染
//! （right_sidebar_visible() 收口），以下测试确保各视图模式的显隐
//! 判定不被回归。

use crate::root::Root;
use crate::sidebar;
use lumino_core::storage::config::UiConfig;
use lumino_message::RightSidebarAction;

/// 默认状态（钢琴卷帘编辑区）：右侧栏应可见
#[test]
fn test_right_sidebar_visible_in_piano_roll_editor() {
    let root = Root::new(&UiConfig::default());

    assert!(root.sidebar.piano_roll_visible, "默认应处于钢琴卷帘编辑区");
    assert!(root.right_sidebar_visible(), "钢琴卷帘编辑区应渲染右侧栏");
}

/// 进入工程走带：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
#[test]
fn test_right_sidebar_hidden_in_arrangement() {
    let mut root = Root::new(&UiConfig::default());
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Project));

    assert!(!root.sidebar.piano_roll_visible, "走带模式钢琴卷帘应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "走带模式右侧栏应跟随钢琴卷帘隐藏"
    );
}

/// 进入瀑布流：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
#[test]
fn test_right_sidebar_hidden_in_waterfall() {
    let mut root = Root::new(&UiConfig::default());
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Waterfall));

    assert!(!root.sidebar.piano_roll_visible, "瀑布流模式钢琴卷帘应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "瀑布流模式右侧栏应跟随钢琴卷帘隐藏"
    );
}

/// 打开音频导出面板：钢琴卷帘区域被面板替代 → 右侧栏隐藏
#[test]
fn test_right_sidebar_hidden_in_audio_export() {
    let mut root = Root::new(&UiConfig::default());
    root.handle_sidebar_event(sidebar::Event::RouteUpdated(sidebar::Route::AudioExport));

    assert!(root.sidebar.audio_export_visible);
    assert!(!root.right_sidebar_visible(), "音频导出面板不应渲染右侧栏");
}

/// 打开视频导出面板：钢琴卷帘区域被面板替代 → 右侧栏隐藏
///
/// 注意：视频导出切换不影响 piano_roll_visible（保持 true），
/// 右侧栏显隐必须显式排除视频导出状态——最容易回归的边界。
#[test]
fn test_right_sidebar_hidden_in_video_export() {
    let mut root = Root::new(&UiConfig::default());
    root.handle_sidebar_event(sidebar::Event::RouteUpdated(sidebar::Route::VideoExport));

    assert!(root.sidebar.video_export_visible);
    assert!(
        root.sidebar.piano_roll_visible,
        "视频导出不影响钢琴卷帘可见性状态（仅视图层区分）"
    );
    assert!(!root.right_sidebar_visible(), "视频导出面板不应渲染右侧栏");
}

/// 关闭钢琴卷帘（点击卷帘切换按钮）：右侧栏跟随隐藏
#[test]
fn test_right_sidebar_hidden_when_piano_roll_closed() {
    let mut root = Root::new(&UiConfig::default());
    root.handle_sidebar_event(sidebar::Event::PianoRollToggled);

    assert!(!root.sidebar.piano_roll_visible, "卷帘切换后应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "钢琴卷帘关闭后右侧栏应跟随隐藏"
    );
}

/// 完整用户流程：打开右侧栏面板 → 进入走带（隐藏）→ 返回钢琴卷帘（恢复）
#[test]
fn test_right_sidebar_hides_with_piano_roll_and_restores() {
    let mut root = Root::new(&UiConfig::default());

    // 打开右侧栏面板（模拟点击图片转 MIDI 按钮）
    root.handle_right_sidebar_action(RightSidebarAction::ImageToMidiClicked);
    assert!(root.right_sidebar.panel_visible, "右侧栏面板应已展开");

    // 进入工程走带：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Project));
    assert!(
        !root.right_sidebar_visible(),
        "离开钢琴卷帘后右侧栏应跟随隐藏"
    );

    // 返回钢琴卷帘：右侧栏随钢琴卷帘 UI 一起恢复
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::PianoRoll));
    assert!(
        root.right_sidebar_visible(),
        "返回钢琴卷帘后右侧栏应恢复显示"
    );
    assert!(
        root.right_sidebar.panel_visible,
        "右侧栏面板展开状态应随钢琴卷帘一起恢复"
    );
}

/// 点击钢琴瀑布流预览按钮：面板互斥切换（展开 → 再点收起）
#[test]
fn test_piano_waterfall_panel_toggle() {
    let mut root = Root::new(&UiConfig::default());
    use crate::right_sidebar::RightSidebarPanel;

    // 初始未激活
    assert!(
        !root
            .right_sidebar
            .is_panel_active(RightSidebarPanel::PianoWaterfall),
        "初始不应处于钢琴瀑布流面板"
    );

    // 第一次点击：展开并切换到钢琴瀑布流
    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);
    assert!(root.right_sidebar.panel_visible, "点击后右侧栏面板应展开");
    assert!(
        root.right_sidebar
            .is_panel_active(RightSidebarPanel::PianoWaterfall),
        "点击后应切换到钢琴瀑布流面板"
    );

    // 第二次点击：互斥收起
    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);
    assert!(
        !root.right_sidebar.panel_visible,
        "再次点击应互斥收起钢琴瀑布流面板"
    );
}

/// 钢琴瀑布流与图片转 MIDI 互斥：打开瀑布流后打开 I2M，active_panel 切换
#[test]
fn test_piano_waterfall_exclusive_with_image_to_midi() {
    let mut root = Root::new(&UiConfig::default());
    use crate::right_sidebar::RightSidebarPanel;

    root.handle_right_sidebar_action(RightSidebarAction::PianoWaterfallClicked);
    assert!(
        root.right_sidebar
            .is_panel_active(RightSidebarPanel::PianoWaterfall),
        "先打开应为钢琴瀑布流面板"
    );

    root.handle_right_sidebar_action(RightSidebarAction::ImageToMidiClicked);
    assert!(
        root.right_sidebar
            .is_panel_active(RightSidebarPanel::ImageToMidi),
        "再打开图片转 MIDI 应互斥切换到该面板"
    );
    assert!(
        !root
            .right_sidebar
            .is_panel_active(RightSidebarPanel::PianoWaterfall),
        "切换后钢琴瀑布流面板应不再激活"
    );
}
