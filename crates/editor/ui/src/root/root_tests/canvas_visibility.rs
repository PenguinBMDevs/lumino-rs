//! 编辑器画布可见性判定（渲染闸门唯一权威源）测试。
//!
//! 背景：卷帘离屏纹理曾在切到导出面板 / 渲染器入口 / 关闭分组后继续渲染，
//! 并被合成到面板下层。`Root::editor_canvas_visible` 是渲染线程
//! `skip_scene_render` 与 UI 侧底层纹理合成的共同判据——本文件锁定各视图状态，
//! 防止新增主区域面板时漏关画布。

use crate::root::Root;
use crate::sidebar;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::UiConfig;

fn root() -> Root {
    Root::new(&UiConfig::default())
}

/// 默认启动：钢琴卷帘可见 → 画布必须渲染。
#[test]
fn test_visible_in_piano_roll() {
    assert!(root().editor_canvas_visible(), "默认应处于卷帘画布");
}

/// 主窗口启动默认态：无面板时显示第一屏；点开卷帘组后第一屏让位。
#[test]
fn test_first_screen_for_startup_empty_state() {
    let mut root = root();
    root.sidebar.enter_first_screen();
    assert!(
        root.shows_first_screen(),
        "启动态（无面板）应显示默认第一屏"
    );
    assert!(!root.editor_canvas_visible(), "第一屏下不应渲染卷帘画布");

    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::PianoRoll));
    assert!(!root.shows_first_screen(), "打开卷帘后第一屏应让位");
    assert!(root.editor_canvas_visible(), "打开卷帘后画布应恢复渲染");
}

/// 工程走带同样由 gfx 画布承载（布局与数据源不同，但仍是同一张离屏纹理）。
#[test]
fn test_visible_in_arrangement() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Project));
    assert!(
        root.editor_canvas_visible(),
        "工程走带必须继续渲染（gfx 画布承载走带布局）"
    );
}

/// 全屏瀑布流：独立外部纹理承载播放器，编辑器画布不参与合成。
#[test]
fn test_hidden_in_waterfall() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Waterfall));
    assert!(
        !root.editor_canvas_visible(),
        "瀑布流模式下卷帘画布必须停绘"
    );
}

/// 渲染器入口（视频剪辑首级面板）：画布被面板替代。
#[test]
fn test_hidden_in_renderer_entry() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Renderer));
    assert!(
        root.is_renderer_entry_active(),
        "断言前提：当前应处于渲染器入口"
    );
    assert!(!root.editor_canvas_visible(), "渲染器入口必须停绘卷帘画布");
}

/// 关闭渲染组（选项卡关闭的落点）：active_group=None + 卷帘隐藏 → 空白主区域。
#[test]
fn test_hidden_after_closing_renderer_group() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Renderer));
    root.handle_sidebar_event(sidebar::Event::GroupToggled(sidebar::GroupId::Renderer));
    assert!(
        root.sidebar.active_group.is_none(),
        "断言前提：关闭后不应残留分组"
    );
    assert!(
        !root.editor_canvas_visible(),
        "关闭分组后主区域为空白，不得残留卷帘画布"
    );
}

/// 音频导出面板：整块替换主区域。
#[test]
fn test_hidden_in_audio_export() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::RouteUpdated(sidebar::Route::AudioExport));
    assert!(root.sidebar.audio_export_visible, "断言前提：面板已打开");
    assert!(!root.editor_canvas_visible(), "音频导出面板下必须停绘");
}

/// 视频导出面板：注意 `piano_roll_visible` 保持 true，必须靠 `is_piano_roll_panel` 排除。
#[test]
fn test_hidden_in_video_export() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::RouteUpdated(sidebar::Route::VideoExport));
    assert!(root.sidebar.video_export_visible, "断言前提：面板已打开");
    assert!(
        root.sidebar.piano_roll_visible,
        "断言前提：视频导出刻意不改卷帘可见性标志"
    );
    assert!(
        !root.editor_canvas_visible(),
        "视频导出面板下必须停绘（只看 piano_roll_visible 会漏判）"
    );
}

/// 关闭卷帘选项卡：UI 隐藏 → 画布停绘。
#[test]
fn test_hidden_when_piano_roll_closed() {
    let mut root = root();
    root.handle_sidebar_event(sidebar::Event::PianoRollToggled);
    assert!(!root.sidebar.piano_roll_visible, "断言前提：卷帘已关闭");
    assert!(!root.editor_canvas_visible(), "卷帘关闭后必须停绘");
}

/// 瀑布流模式优先于走带路由：`current_mode` 是更高阶的闸门。
#[test]
fn test_waterfall_mode_excludes_arrangement_route() {
    let mut root = root();
    root.sidebar.route = sidebar::Route::Arrangement;
    root.state.current_mode = AppMode::Waterfall;
    assert!(
        !root.editor_canvas_visible(),
        "瀑布流模式下即使路由仍是走带，也不得渲染卷帘画布"
    );
}
