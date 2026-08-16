use super::*;
use crate::message::{CustomPrecisionAction, Message};
use crate::root::Root;
use lumino_core::storage::config::UiConfig;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 挂载测试 document 到 Root（当前轨 = 1，音符写入 document 单一权威源）
fn attach_test_document(root: &mut Root) {
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
}

#[test]
fn test_message_router_consumes_message() {
    struct ConsumingHandler;
    impl MessageHandler for ConsumingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            None
        }
    }

    let mut router = MessageRouter::new();
    router.register(Box::new(ConsumingHandler));
    let mut root = create_root();

    // 不应 panic；消息被消费后不再继续传递
    router.route(&mut root, Message::ToggleSettings);
}

#[test]
fn test_message_router_falls_through_when_not_consumed() {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct PassThroughHandler;
    impl MessageHandler for PassThroughHandler {
        fn handle(&mut self, _root: &mut Root, msg: Message) -> Option<Message> {
            Some(msg)
        }
    }

    let received: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    struct CapturingHandler {
        received: Rc<RefCell<bool>>,
    }
    impl MessageHandler for CapturingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            *self.received.borrow_mut() = true;
            None
        }
    }

    let mut router = MessageRouter::new();
    let received2 = Rc::clone(&received);
    router.register(Box::new(PassThroughHandler));
    router.register(Box::new(CapturingHandler {
        received: received2,
    }));

    let mut root = create_root();
    router.route(&mut root, Message::ToggleSettings);

    // 由于第一个 handler 返回 Some，消息应落到第二个 handler
    assert!(*received.borrow(), "消息应透传到第二个处理器");
}

/// BUG 回归：无需重绘的 Sidebar 消息必须被判定为"已处理"。
///
/// 修复前 `try_handle_direct` 把 `handle_sidebar_event` 的"是否需要重绘"
/// 返回值当作"是否已处理"，导致 hover 移动（TrackReorderMoved）、重命名
/// 输入（TrackRenameChanged）等无状态变化事件误落入 MessageRouter，
/// 在 router 尾部刷"未处理的消息"噪音 WARN。
#[test]
fn test_sidebar_message_always_handled_directly() {
    let mut root = create_root();

    // 典型高频噪音源：音轨列表 hover 移动（未按下时 track_reorder 为 None，
    // 状态零变化 → 无需重绘）
    assert!(root.try_handle_direct(&Message::Sidebar(
        crate::sidebar::Event::TrackReorderMoved { x: 1.0, y: 1.0 },
    )));
    // 典型高频噪音源：重命名输入（只改 buffer，不触发重绘判定）
    assert!(root.try_handle_direct(&Message::Sidebar(
        crate::sidebar::Event::TrackRenameChanged(0, "New Name".into()),
    )));
    // 正常 Sidebar 事件同样必须已处理
    assert!(root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackSelected(0),)));
}

#[test]
fn test_collaboration_handler_opens_dialog() {
    let mut handler = CollaborationHandler::new();
    let mut root = create_root();

    assert!(!root.state.collaboration_dialog.is_open);
    handler.handle(
        &mut root,
        Message::Collaboration(lumino_message::CollaborationAction::OpenDialog),
    );
    assert!(root.state.collaboration_dialog.is_open);
}

#[test]
fn test_dialog_handler_opens_custom_precision() {
    let mut handler = DialogHandler::new();
    let mut root = create_root();

    let _ = crate::event::take_events();
    let result = handler.handle(
        &mut root,
        Message::CustomPrecision(CustomPrecisionAction::OpenDialog),
    );
    assert!(result.is_none(), "处理器应消费消息");

    let emitted = crate::event::take_events();
    let has_open_event = emitted.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(
                crate::event::window::dialog::Event::OpenCustomPrecisionDialog
            ))
        )
    });
    assert!(has_open_event, "应发射 OpenCustomPrecisionDialog 窗口事件");
}

#[test]
fn test_toolbar_handler_play_creates_manager() {
    let mut handler = ToolbarHandler::new();
    let mut root = create_root();

    // 添加一个音符，使播放管理器能够初始化（document 唯一权威源）
    attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        crate::editor::note::Note::new(0.0, 60, 480.0),
    );

    assert!(root.playback.manager.is_none());
    handler.handle(&mut root, Message::Toolbar(crate::toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "Play 消息应创建播放管理器");
    assert!(root.toolbar.is_playing);
}

#[test]
fn test_handle_core_event_re_emits_event() {
    let mut root = create_root();

    // 清空已有事件
    let _ = crate::event::take_events();

    let event = crate::event::Event::menu_file(crate::event::menu::file::Event::New);
    root.handle_core_event(event.clone());

    let emitted = crate::event::take_events();
    assert!(
        emitted
            .iter()
            .any(|e| format!("{:?}", e) == format!("{:?}", event)),
        "handle_core_event 应重新发出传入的事件"
    );
}

#[test]
fn test_playhead_actions_do_not_change_notes() {
    let mut root = create_root();

    // 演奏指示线移动不应被识别为音符变化
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::Scrubbed { tick: 100.0 }),
        "Scrubbed 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::IndicatorDragStart { x: 50.0 }),
        "IndicatorDragStart 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::IndicatorDragMove { x: 60.0 }),
        "IndicatorDragMove 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::Scrolled {
            delta_x: 10.0,
            delta_y: 0.0,
        }),
        "Scrolled 不应改变音符"
    );
}

#[test]
fn test_piano_roll_context_menu_open_close() {
    let mut root = create_root();

    // 初始状态菜单关闭
    assert!(!root.editor.context_menu.open);
    assert!(root.editor.context_menu.position.is_none());

    // 打开菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(120.0, 80.0),
        },
    ));
    assert!(root.editor.context_menu.open);
    assert_eq!(
        root.editor.context_menu.position,
        Some(iced_core::Point::new(120.0, 80.0))
    );

    // 关闭菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Close,
    ));
    assert!(!root.editor.context_menu.open);
    assert!(root.editor.context_menu.position.is_none());
}

#[test]
fn test_piano_roll_context_menu_item_click_closes_and_dispatches() {
    let mut root = create_root();

    // 打开菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(100.0, 100.0),
        },
    ));
    assert!(root.editor.context_menu.open);

    // 添加一个音符，使全选有意义（document 唯一权威源）
    attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        crate::editor::note::Note::new(0.0, 60, 480.0),
    );

    // 点击全选：菜单关闭且音符被选中
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::SelectAll,
        ),
    ));
    assert!(!root.editor.context_menu.open);
    assert_eq!(root.editor.editor_state.interaction.selected_notes.len(), 1);
}

// ===== 力度面板双向滚轮测试 =====

use crate::editor::velocity::EditMode;
use crate::message::VelocityAction;

/// 双向滚轮（对角线）：水平分量滚动时间轴，垂直分量滚动自动化曲线，同时生效
#[test]
fn test_velocity_wheel_scrolled_bidirectional() {
    let mut root = create_root();
    // 水平滚动需要横向内容空间（与网格测试一致）
    root.editor.editor_state.canvas.size_x = 1000.0;
    // 垂直滚动需要 zoom > 1 才有滚动余量（默认 zoom=1.0 时可见范围=满量程，会被 clamp 到 0）
    root.editor.velocity_panel.value_zoom = 2.0;
    root.editor.velocity_panel.edit_mode = EditMode::Cc(1);

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: -1.0, // 上滑 → 自动化曲线 value_scroll 增大
        }),
    );

    // 水平：左滑 → scroll_x 增大（内容跟随手指）
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量应滚动时间轴，target_x={}",
        root.editor.editor_state.view.smooth_scroll.target_x
    );
    // 垂直：自动化曲线滚动（CC 模式生效）
    assert!(
        root.editor.velocity_panel.value_scroll > 0.0,
        "垂直分量应滚动自动化曲线，value_scroll={}",
        root.editor.velocity_panel.value_scroll
    );
}

/// 双向滚轮：Velocity 模式垂直分量不生效（保持无操作语义），水平分量仍生效
#[test]
fn test_velocity_wheel_scrolled_vertical_ignored_in_velocity_mode() {
    let mut root = create_root();
    root.editor.editor_state.canvas.size_x = 1000.0;
    root.editor.velocity_panel.edit_mode = EditMode::Velocity;

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: 1.0,
        }),
    );

    assert_eq!(
        root.editor.velocity_panel.value_scroll, 0.0,
        "Velocity 模式垂直分量应被忽略"
    );
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量仍应滚动时间轴"
    );
}

/// 构造已打开的找回删除音轨对话框（含 2 个缓存条目，默认选中第一个）
fn setup_recover_track_dialog() -> Root {
    let mut root = create_root();
    root.set_recover_track_dialog_open(true);
    root.set_recover_track_dialog_entries(vec![
        crate::state::root_state::RecoverTrackEntry {
            path: "C:\\cache\\a.lmdeltrack".into(),
            filename: "a.lmdeltrack".into(),
            track_id: 1,
            track_name: "A".into(),
            port: 0,
            channel: 1,
            note_count: 10,
            deleted_at: "ts:1".into(),
            original_index: 0,
        },
        crate::state::root_state::RecoverTrackEntry {
            path: "C:\\cache\\b.lmdeltrack".into(),
            filename: "b.lmdeltrack".into(),
            track_id: 2,
            track_name: "B".into(),
            port: 0,
            channel: 2,
            note_count: 20,
            deleted_at: "ts:2".into(),
            original_index: 1,
        },
    ]);
    root
}

/// 永久删除：对话框必须保持开启（bug 回归），并产出结果转交 Runner 执行磁盘销毁
#[test]
fn test_recover_track_permanent_delete_keeps_dialog_open() {
    let mut root = setup_recover_track_dialog();
    let mut handler = DialogHandler::new();

    let result = handler.handle(
        &mut root,
        Message::RecoverTrack(lumino_message::RecoverTrackAction::PermanentlyDelete {
            path: "C:\\cache\\a.lmdeltrack".into(),
            track_id: 1,
        }),
    );

    assert!(result.is_none(), "处理器应消费消息");
    assert!(
        root.state.recover_track_dialog.is_open,
        "永久删除后对话框应保持开启，支持连续操作"
    );
    assert!(
        matches!(
            root.state.dialog_result,
            Some(crate::host::DialogResult::RecoverTrackPermanentlyDelete { track_id: 1, .. })
        ),
        "应产出 RecoverTrackPermanentlyDelete 结果转交 Runner"
    );
}

/// 恢复：对话框仍应关闭（行为不变，回归保护）
#[test]
fn test_recover_track_restore_closes_dialog() {
    let mut root = setup_recover_track_dialog();
    let mut handler = DialogHandler::new();

    let result = handler.handle(
        &mut root,
        Message::RecoverTrack(lumino_message::RecoverTrackAction::Restore {
            path: "C:\\cache\\a.lmdeltrack".into(),
            original_index: 0,
        }),
    );

    assert!(result.is_none(), "处理器应消费消息");
    assert!(
        !root.state.recover_track_dialog.is_open,
        "恢复后对话框应关闭"
    );
    assert!(
        matches!(
            root.state.dialog_result,
            Some(crate::host::DialogResult::RecoverTrackRestore { .. })
        ),
        "应产出 RecoverTrackRestore 结果转交 Runner"
    );
}

// ── 新建音轨后 document 同步（BUG 回归：新音轨无法放置音符） ─────────────

/// 读取 document 当前音轨数（无 document 时为 0）
fn doc_track_count(root: &Root) -> usize {
    root.editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|d| d.track_count())
        .unwrap_or(0)
}

/// 核心 BUG 复现：新建音轨 → 切换到新轨 → 放置音符。
///
/// 修复前：`AddTrack` 只更新 sidebar.tracks（UI 列表），`MidiDocument.notes`
/// 未同步扩轨；新轨 `insert_note` 因 track_id 越界静默返回 false，音符被丢弃，
/// 表现为"只能在第一个音轨放置音符"。
#[test]
fn test_add_track_expands_document_for_note_placement() {
    let mut root = create_root();
    attach_test_document(&mut root); // document 2 轨，sidebar 默认 2 轨，current_track=1

    // 用户操作：新建音轨
    root.handle_sidebar_event(crate::sidebar::Event::AddTrack);

    // document 必须同步扩展为 3 轨
    assert_eq!(
        doc_track_count(&root),
        3,
        "AddTrack 后 document 应扩展为 3 轨"
    );

    // 新音轨（id=2）必须能插入音符——修复前此处静默失败
    let new_id = root
        .sidebar
        .tracks
        .last()
        .map(|t| t.id)
        .expect("AddTrack 后 sidebar 应包含新音轨");
    assert_eq!(new_id, 2, "新音轨 id 应为 2");
    let inserted = root
        .editor
        .editor_state
        .data
        .insert_note(new_id, crate::editor::note::Note::new(0.0, 60, 480.0));
    assert!(inserted, "新音轨应能插入音符");
    assert_eq!(
        root.editor.editor_state.data.track_notes(new_id).len(),
        1,
        "新音轨应包含 1 个音符"
    );
}

/// 同类路径：在指定音轨上方/下方添加音轨，document 同样需要扩轨
#[test]
fn test_track_add_above_below_expands_document() {
    let mut root = create_root();
    attach_test_document(&mut root);

    root.handle_sidebar_event(crate::sidebar::Event::TrackAddAbove(1));
    root.handle_sidebar_event(crate::sidebar::Event::TrackAddBelow(1));

    // 两次添加：sidebar 4 轨（0/1/2/3），document 必须覆盖到最大 id
    assert_eq!(
        doc_track_count(&root),
        4,
        "添加上/下方音轨后 document 应扩展为 4 轨"
    );

    let ids: Vec<usize> = root.sidebar.tracks.iter().map(|t| t.id).collect();
    for id in ids {
        let inserted = root
            .editor
            .editor_state
            .data
            .insert_note(id, crate::editor::note::Note::new(0.0, 60, 480.0));
        assert!(inserted, "音轨 id={} 应能插入音符", id);
    }
}

/// 同类路径：协作远程音轨加入后，document 必须扩轨（此前只 push sidebar.tracks）
#[test]
fn test_add_remote_track_expands_document() {
    let mut root = create_root();
    attach_test_document(&mut root);

    root.add_remote_track(5);

    assert_eq!(doc_track_count(&root), 6, "协作远程音轨应同步扩展 document");
    let inserted = root
        .editor
        .editor_state
        .data
        .insert_note(5, crate::editor::note::Note::new(0.0, 60, 480.0));
    assert!(inserted, "协作远程音轨应能插入音符");
}

/// 同类路径：恢复已删除音轨（track_id 可能大于当前 document 轨数）
#[test]
fn test_apply_track_restored_expands_document() {
    use lumino_event::window::track::{TrackDeletionNote, TrackDeletionPayload};

    let mut root = create_root();
    attach_test_document(&mut root);

    let payload = TrackDeletionPayload {
        track_id: 5,
        track_name: "Restored".to_string(),
        port: 0,
        channel: 0,
        is_drum: false,
        max_tick: 480,
        original_index: 1,
        notes: vec![TrackDeletionNote {
            start_tick: 0,
            end_tick: 480,
            key: 60,
            velocity: 100,
            channel: 0,
            port: 0,
        }],
    };
    root.apply_track_restored(payload);

    assert_eq!(
        doc_track_count(&root),
        6,
        "恢复音轨后 document 应扩展为 6 轨"
    );
    assert_eq!(
        root.editor.editor_state.data.track_notes(5).len(),
        1,
        "恢复的音符应写入 document"
    );
}

// ── Tempo 面板 BPM 上限（BUG 回归：硬编码 10000 截断） ──────────────────
/// BUG 复现：用户把 Tempo 面板绘制上限（tempo_max_bpm，设置里可调至 65536）
/// 调高后，拖拽速度点仍被旧硬编码 `clamp(20.0, 10000.0)` 截断，
/// 曲线永远无法到达面板顶部，表现为"最大绘制值只能到 10000"。
///
/// 修复前：面板上限 20000 时，拖到 30000 只能得到 10000。
#[test]
fn test_tempo_drag_move_uses_panel_max_bpm() {
    let mut root = create_root();
    root.editor.velocity_panel.tempo_max_bpm = 20000.0;

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 30000.0));

    let bpm = root.editor.editor_state.data.tempo_points[0].bpm;
    assert_eq!(
        bpm, 20000.0,
        "拖拽值应截断到面板绘制上限，而非旧硬编码 10000"
    );
}

/// 同类路径：新建速度点同样按面板绘制上限截断
#[test]
fn test_tempo_add_uses_panel_max_bpm() {
    let mut root = create_root();
    root.editor.velocity_panel.tempo_max_bpm = 20000.0;

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoAdd(480.0, 50000.0));

    let bpm = root
        .editor
        .editor_state
        .data
        .tempo_points
        .iter()
        .find(|p| (p.tick - 480.0).abs() < f32::EPSILON)
        .map(|p| p.bpm)
        .expect("TempoAdd 后应存在 tick=480 的速度点");
    assert_eq!(bpm, 20000.0, "新建点应截断到面板绘制上限");
}

/// 默认上限 512 时行为保持不变：超出上限的值截断到 512
#[test]
fn test_tempo_clamp_uses_default_max_bpm() {
    let mut root = create_root();

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 600.0));

    assert_eq!(
        root.editor.editor_state.data.tempo_points[0].bpm, 512.0,
        "默认上限 512 下 600 应截断到 512"
    );
}

/// 下限保持 TEMPO_BPM_MIN（20）：低于下限的值截断到 20
#[test]
fn test_tempo_clamp_min_bpm() {
    let mut root = create_root();

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 5.0));

    assert_eq!(
        root.editor.editor_state.data.tempo_points[0].bpm, 20.0,
        "低于下限的值应截断到 20"
    );
}

// ── PPQ 修改贯穿保存链路（BUG 回归：工程文件落盘旧值 480） ──────────────

/// BUG 复现：工具栏修改 PPQ 只更新视图状态，`document.division`（单一权威源）
/// 保持旧值 480；保存工程时 `from_midi_document` 读取 document.division，
/// 导致新工程 PPQ 丢失、工程文件永远落盘 480。
#[test]
fn test_set_ppq_syncs_document_division() {
    let mut root = create_root();
    attach_test_document(&mut root);

    // 初始：视图默认 1920，测试文档构造为 480（真实场景下新工程空文档
    // 与视图同源，此处故意制造不一致以验证 set_ppq 能把 document 拉齐）
    assert_eq!(root.editor.editor_state.view.ppq, 1920);
    assert_eq!(
        root.editor
            .editor_state
            .data
            .document
            .as_ref()
            .expect("测试文档应已挂载")
            .division,
        480
    );

    // 用户经工具栏把 PPQ 改为 960
    root.set_ppq(960);

    // 视图状态同步
    assert_eq!(root.editor.editor_state.view.ppq, 960);
    // 保存链路权威源必须同步——修复前此处保持 480，工程文件落盘错误
    assert_eq!(
        root.editor
            .editor_state
            .data
            .document
            .as_ref()
            .expect("测试文档应已挂载")
            .division,
        960,
        "document.division 应随 PPQ 修改同步，保证工程文件保存新 PPQ"
    );
}

/// 无 document 时（编辑器已重置、空白工程未初始化）set_ppq 不应 panic
#[test]
fn test_set_ppq_without_document_no_panic() {
    let mut root = create_root();

    root.set_ppq(960);

    assert_eq!(root.editor.editor_state.view.ppq, 960);
    assert!(root.editor.editor_state.data.document.is_none());
}
