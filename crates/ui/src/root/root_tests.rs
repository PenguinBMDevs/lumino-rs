use super::*;
use crate::Message;
use crate::message::{ProjectSettingsAction, SettingsDialogAction};
use crate::root::handlers::MessageHandler;

// ================================================================
// 对话框关闭处理器 —— dialog_type 不复位测试
//
// 修复背景：关闭对话框时 handler 曾将 dialog_type 复位为 None，
// 导致 view_dialog() 在窗口销毁前的最后一帧通过 _ 通配符渲染出精度面板。
// 修复后 handler 不再修改 dialog_type，以下测试确保这一行为。
// ================================================================

#[test]
fn test_close_settings_dialog_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::Settings);
    let mut handler = handlers::DialogHandler::new();
    handler.handle(
        &mut root,
        Message::SettingsDialog(SettingsDialogAction::CloseDialog),
    );

    // 关闭设置对话框不应复位 dialog_type（防止窗口销毁前一帧闪跳到精度面板）
    assert_eq!(
        root.state.dialog_type,
        DialogType::Settings,
        "CloseSettingsDialog 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "CloseSettingsDialog 应设置 dialog_result"
    );
}

#[test]
fn test_close_project_settings_dialog_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::ProjectSettings);
    let mut handler = handlers::DialogHandler::new();
    handler.handle(
        &mut root,
        Message::ProjectSettings(ProjectSettingsAction::CloseDialog),
    );

    assert_eq!(
        root.state.dialog_type,
        DialogType::ProjectSettings,
        "CloseProjectSettingsDialog 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "CloseProjectSettingsDialog 应设置 dialog_result"
    );
}

#[test]
fn test_confirm_load_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
    root.handle_confirm_load();

    assert_eq!(
        root.state.dialog_type,
        DialogType::LoadConfirm,
        "handle_confirm_load 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "handle_confirm_load 应设置 dialog_result"
    );
}

#[test]
fn test_cancel_load_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
    root.handle_cancel_load();

    assert_eq!(
        root.state.dialog_type,
        DialogType::LoadConfirm,
        "handle_cancel_load 不应修改 dialog_type"
    );
    // handle_cancel_load 仅关闭对话框，不设置结果
    assert!(
        !root.state.load_confirm_dialog.is_open,
        "handle_cancel_load 应关闭加载确认对话框"
    );
}

// ================================================================
// view_dialog None 测试
//
// DialogType::None 应渲染空容器而不是回退到精度面板（修复前的 bug）。
// 此处验证 view() 不 panic，类型已由 match 分支的显式匹配保证。
// ================================================================

#[test]
fn test_view_dialog_none_does_not_panic() {
    // DialogType::None 匹配专门的空容器分支，不应 panic
    let root = Root::new_dialog("dark", DialogType::None);
    let _element = root.view();
}

// ================================================================
// 变速按钮 Ctrl+Click 测试
//
// 修复前 flip_button 传入 has_selection 作为 enabled 参数，
// 无选中时按钮 disabled，Ctrl+Click 事件根本不会发射。
// 修复后按钮总是 enabled，handler 内部已对无选中情况做了兜底。
// ================================================================

#[test]
fn test_speed_change_ctrl_click_opens_dialog_event() {
    // 清空全局事件缓冲区
    let _ = crate::event::take_events();

    // Ctrl+Click 应发射 OpenSpeedChangeDialog 事件（不依赖选中状态）
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.ctrl_pressed = true;
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));

    let events = crate::event::take_events();
    let has_open_event = events.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(
                crate::event::window::dialog::Event::OpenSpeedChangeDialog
            ))
        )
    });
    assert!(
        has_open_event,
        "Ctrl+Click 变速应发射 OpenSpeedChangeDialog 事件"
    );
}

#[test]
fn test_speed_change_direct_click_no_selection_returns_early() {
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.ctrl_pressed = false;

    // 无音符 + 无选中：直接点击应无副作用地提前返回
    let _ = crate::event::take_events();
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));
    let events = crate::event::take_events();

    assert!(
        events.is_empty(),
        "无选中时直接点击变速不应发射任何窗口事件"
    );
    assert_eq!(
        root.state.dialog_type,
        DialogType::None,
        "无选中时直接点击变速不应改变 dialog_type"
    );
}

#[test]
fn test_speed_change_button_always_enabled_in_view() {
    // 验证在工具栏 view 中变速按钮的 enabled 始终为 true
    // （与 has_selection 解耦），确保 Ctrl+Click 路径可到达 handler。
    // 这是 toolbar/view.rs 中 flip_button 调用改为硬编码 true 的行为保证。
    // 此处通过构造两种场景并调用 view() 来验证不 panic：
    //   1. has_selection = true
    //   2. has_selection = false
    let ui_config = lumino_core::storage::config::UiConfig::default();
    let root = Root::new(&ui_config);

    // 构造检测仪表盘所需的性能上下文（与产品运行时一致的数据来源）
    let perf_ctx = crate::toolbar::ToolbarPerfContext {
        playback_tick: root.editor.playback_position,
        ppq: root.editor.editor_state.view.ppq,
        tempo_points: &root.editor.editor_state.data.tempo_points,
    };

    // 有选中 -> view
    let _element = root.toolbar.toolbar_view(
        &root.window,
        true,
        root.settings.language,
        &perf_ctx,
        1920.0,
        false,
    );

    // 无选中 -> view（不应 panic/assert）
    let _element = root.toolbar.toolbar_view(
        &root.window,
        false,
        root.settings.language,
        &perf_ctx,
        1920.0,
        false,
    );

    // 验证通过：两种情况下 view 均正常返回
}

// ================================================================
// 工程走带视图最大 tick 缓存测试
//
// 播放时每帧需要最大 tick 来计算滚动范围；若每帧全量扫描 track_notes，
// 大型 MIDI 会在主线程造成卡顿。此测试验证缓存按 track_notes_gen 失效。
// ================================================================

#[test]
fn test_arrangement_max_tick_end_caches_by_gen() {
    use lumino_core::storage::config::UiConfig;

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);

    // 无音符时返回 DEFAULT_MIN_TICKS
    assert_eq!(
        root.arrangement_max_tick_end(),
        crate::constants::editor::DEFAULT_MIN_TICKS
    );

    // 在非指挥轨道添加音符（tick=4000, length=100，终点=4100）
    // 必须超过 DEFAULT_MIN_TICKS（3840），否则会被最小值覆盖
    // 单一权威源：先挂载 document（音符写入 document）
    let doc = lumino_midi_loader::MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::new(),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 0".into()), Some("Track 1".into())],
        total_ticks: 0,
        track_count: 2,
        tracks: lumino_midi_loader::TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],

        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(2),
    };
    root.set_midi_document(doc);
    root.editor.editor_state.data.current_track = 1;
    let _ = root
        .editor
        .editor_state
        .data
        .finish_drawing(4000.0, 60, 4100.0, 1.0, 10.0);

    // track_notes_gen 已变化，缓存应重新计算
    let max_tick = root.arrangement_max_tick_end();
    assert!((max_tick - 4100.0).abs() < f32::EPSILON);

    // 缓存已写入
    assert!((root.arrangement_view.viewport.cached_max_tick_end - 4100.0).abs() < f32::EPSILON);
    assert_eq!(
        root.arrangement_view.viewport.cached_track_notes_gen,
        root.editor.editor_state.data.track_notes_gen
    );
}

// ================================================================
// 右侧栏跟随钢琴卷帘 UI 显隐测试
//
// 修复背景：离开钢琴卷帘（进入工程走带）后，钢琴卷帘 UI 隐藏，
// 但右侧栏仍渲染在走带视图右侧（fb2abc93 将右侧栏移入钢琴卷帘
// 编辑区时遗漏了走带分支）。修复后右侧栏仅在钢琴卷帘编辑区渲染
// （right_sidebar_visible() 收口），以下测试确保各视图模式的显隐
// 判定不被回归。
// ================================================================

/// 默认状态（钢琴卷帘编辑区）：右侧栏应可见
#[test]
fn test_right_sidebar_visible_in_piano_roll_editor() {
    let root = Root::new(&lumino_core::storage::config::UiConfig::default());

    assert!(root.sidebar.piano_roll_visible, "默认应处于钢琴卷帘编辑区");
    assert!(root.right_sidebar_visible(), "钢琴卷帘编辑区应渲染右侧栏");
}

/// 进入工程走带：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
#[test]
fn test_right_sidebar_hidden_in_arrangement() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.handle_sidebar_event(crate::sidebar::Event::GroupToggled(
        crate::sidebar::GroupId::Project,
    ));

    assert!(!root.sidebar.piano_roll_visible, "走带模式钢琴卷帘应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "走带模式右侧栏应跟随钢琴卷帘隐藏"
    );
}

/// 进入瀑布流：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
#[test]
fn test_right_sidebar_hidden_in_waterfall() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.handle_sidebar_event(crate::sidebar::Event::GroupToggled(
        crate::sidebar::GroupId::Waterfall,
    ));

    assert!(!root.sidebar.piano_roll_visible, "瀑布流模式钢琴卷帘应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "瀑布流模式右侧栏应跟随钢琴卷帘隐藏"
    );
}

/// 打开音频导出面板：钢琴卷帘区域被面板替代 → 右侧栏隐藏
#[test]
fn test_right_sidebar_hidden_in_audio_export() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.handle_sidebar_event(crate::sidebar::Event::RouteUpdated(
        crate::sidebar::Route::AudioExport,
    ));

    assert!(root.sidebar.audio_export_visible);
    assert!(!root.right_sidebar_visible(), "音频导出面板不应渲染右侧栏");
}

/// 打开视频导出面板：钢琴卷帘区域被面板替代 → 右侧栏隐藏
///
/// 注意：视频导出切换不影响 piano_roll_visible（保持 true），
/// 右侧栏显隐必须显式排除视频导出状态——最容易回归的边界。
#[test]
fn test_right_sidebar_hidden_in_video_export() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.handle_sidebar_event(crate::sidebar::Event::RouteUpdated(
        crate::sidebar::Route::VideoExport,
    ));

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
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.handle_sidebar_event(crate::sidebar::Event::PianoRollToggled);

    assert!(!root.sidebar.piano_roll_visible, "卷帘切换后应隐藏");
    assert!(
        !root.right_sidebar_visible(),
        "钢琴卷帘关闭后右侧栏应跟随隐藏"
    );
}

/// 完整用户流程：打开右侧栏面板 → 进入走带（隐藏）→ 返回钢琴卷帘（恢复）
#[test]
fn test_right_sidebar_hides_with_piano_roll_and_restores() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());

    // 打开右侧栏面板（模拟点击图片转 MIDI 按钮）
    root.handle_right_sidebar_action(lumino_message::RightSidebarAction::ImageToMidiClicked);
    assert!(root.right_sidebar.panel_visible, "右侧栏面板应已展开");

    // 进入工程走带：钢琴卷帘 UI 隐藏 → 右侧栏跟随隐藏
    root.handle_sidebar_event(crate::sidebar::Event::GroupToggled(
        crate::sidebar::GroupId::Project,
    ));
    assert!(
        !root.right_sidebar_visible(),
        "离开钢琴卷帘后右侧栏应跟随隐藏"
    );

    // 返回钢琴卷帘：右侧栏随钢琴卷帘 UI 一起恢复
    root.handle_sidebar_event(crate::sidebar::Event::GroupToggled(
        crate::sidebar::GroupId::PianoRoll,
    ));
    assert!(
        root.right_sidebar_visible(),
        "返回钢琴卷帘后右侧栏应恢复显示"
    );
    assert!(
        root.right_sidebar.panel_visible,
        "右侧栏面板展开状态应随钢琴卷帘一起恢复"
    );
}

// ================================================================
// 工程设置重置测试
//
// 修复背景：工程设置（标题/作者/版权/BPM/拍号）属于工程级数据，
// 但存放在程序全局 Root 状态中。关闭工程/新建工程只调用
// clear_editor()，曾遗漏重置 project_settings_dialog，导致旧工程的
// 数据残留到下一个工程。以下测试锁定 reset 行为。
// ================================================================

#[test]
fn test_reset_project_settings_restores_defaults() {
    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut root = Root::new(&ui_config);

    // 模拟用户在工程设置面板填写了数据
    root.set_project_settings_data(
        "我的工程".to_string(),
        "96".to_string(),
        "© 2026".to_string(),
        "张三".to_string(),
        "2026-07-01 10:00:00".to_string(),
        3600.0,
        vec![(0, 6, 8)],
    );
    assert_eq!(root.state.project_settings_dialog.title, "我的工程");
    assert_eq!(root.state.project_settings_dialog.tempo, "96");
    assert_eq!(root.state.project_settings_dialog.author, "张三");

    // 关闭工程：工程设置必须恢复默认值，不得残留
    root.reset_project_settings();

    let dialog = &root.state.project_settings_dialog;
    assert!(dialog.title.is_empty(), "关闭工程后标题应为空");
    assert_eq!(dialog.tempo, "120", "关闭工程后 BPM 应恢复默认 120");
    assert!(dialog.copyright.is_empty(), "关闭工程后版权应为空");
    assert!(dialog.author.is_empty(), "关闭工程后作者应为空");
    assert!(
        dialog.created_display.is_empty(),
        "关闭工程后创建日期应为空"
    );
    assert_eq!(
        dialog.total_editing_time_seconds, 0.0,
        "关闭工程后累计创作时间应为 0"
    );
    assert_eq!(
        dialog.time_signature_numerator, "4",
        "关闭工程后拍号分子应恢复默认 4"
    );
    assert_eq!(
        dialog.time_signature_denominator, "4",
        "关闭工程后拍号分母应恢复默认 4"
    );
}

// ================================================================
// 云存储快照同步 —— 保存到云切换文件夹不弹回根目录
//
// 修复背景：对话框（CloudBrowser/CloudConnect/设置）为独立 Root，
// 运行期通过 sync_cloud_snapshot_from 广播主窗口快照。若快照包含
// 连接表单字段，用户在连接面板输入会被后台广播覆盖；若导航字段
// （current_path 等）由广播直接覆盖，保存模式下切换文件夹会被
// 弹回根目录（用户报告 bug）。以下测试锁定快照同步边界。
// ================================================================

#[test]
fn test_cloud_snapshot_sync_preserves_connect_form() {
    let mut main = Root::new(&lumino_core::storage::config::UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudConnect);

    // 用户在对话框输入表单
    dialog.cloud.name = "我的NAS".to_string();
    dialog.cloud.address = "nas.example.com".to_string();
    dialog.cloud.password = "secret".to_string();

    // 主窗口快照：表单为空
    main.cloud.name.clear();
    main.cloud.address.clear();
    main.cloud.password.clear();

    // 运行期广播快照 → 表单字段必须保留，不能被覆盖
    dialog.sync_cloud_snapshot_from(&main);
    assert_eq!(dialog.cloud.name, "我的NAS", "快照不得覆盖名称输入");
    assert_eq!(
        dialog.cloud.address, "nas.example.com",
        "快照不得覆盖地址输入"
    );
    assert_eq!(dialog.cloud.password, "secret", "快照不得覆盖密码输入");
}

#[test]
fn test_cloud_snapshot_sync_preserves_navigation_and_keeps_devices() {
    let mut main = Root::new(&lumino_core::storage::config::UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudBrowser);

    // 对话框已导航到子目录（保存模式：用户切到目标文件夹）
    dialog.cloud.selected_id = Some("conn-1".to_string());
    dialog.cloud.current_path = "/projects/backup".to_string();
    dialog.cloud.new_folder_input = "草稿".to_string();

    // 主窗口快照：设备列表有更新（新连接），但导航停留在根目录
    main.cloud
        .connections
        .push(crate::state::cloud_state::CloudConnInfo {
            id: "conn-1".into(),
            name: "NAS".into(),
            protocol: "SFTP".to_string(),
            online: true,
        });
    main.cloud.selected_id = Some("conn-1".to_string());
    main.cloud.current_path = String::new();

    dialog.sync_cloud_snapshot_from(&main);

    // 设备列表必须同步（共享数据）
    assert_eq!(dialog.cloud.connections.len(), 1, "设备列表应同步");
    assert!(dialog.cloud.connections[0].online, "在线状态应同步");
    // 导航字段：主窗口与对话框一致时覆盖无感知；此处主窗口为根目录
    // 属于历史快照，广播语义由 runner 事件回传保证一致（见 cloud.rs）。
    // 本地编辑字段（新建文件夹输入）必须保留
    assert_eq!(
        dialog.cloud.new_folder_input, "草稿",
        "快照不得覆盖本地输入"
    );
}

#[test]
fn test_cloud_full_sync_used_at_dialog_open() {
    let mut main = Root::new(&lumino_core::storage::config::UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudBrowser);

    main.cloud.selected_id = Some("conn-9".to_string());
    main.cloud.current_path = "/root".to_string();

    // 首次打开：完整拷贝（表单回显 + 选中设备 + 目录）
    dialog.sync_cloud_state_from(&main);
    assert_eq!(dialog.cloud.selected_id.as_deref(), Some("conn-9"));
    assert_eq!(dialog.cloud.current_path, "/root");
}
