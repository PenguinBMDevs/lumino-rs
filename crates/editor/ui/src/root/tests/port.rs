//! Root 侧音轨端口编辑写回测试（sidebar → document / 输出布局）

use crate::root::Root;
use crate::test_helpers::event_queue_lock;
use lumino_core::storage::config::UiConfig;

/// 端口编辑写回文档 + 置脏 + 视觉顺序同步；max_port 变化时发出布局变更事件
#[test]
fn test_forward_port_change_writes_document_and_notifies_layout() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events(); // 清掉并行测试残留事件

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.set_midi_document(crate::test_helpers::make_test_document());

    // 模拟 sidebar 已即时更新：track 1 端口 0 -> 3（pending 携带旧值供回滚）
    root.sidebar.tracks[1].port = 3;
    root.sidebar.tracks[1].display_label = crate::sidebar::Sidebar::track_label(3, 0);
    root.sidebar.pending_track_port_change = Some((1, 0, 3));

    root.forward_pending_track_port_change();

    let data = &root.editor.editor_state.data;
    assert_eq!(
        data.document.as_ref().expect("文档应存在").track_ports[1],
        3,
        "端口应写回 document.track_ports"
    );
    assert!(data.modified, "端口编辑应置工程脏");
    assert_eq!(
        data.track_visual_order,
        root.sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        "视觉顺序映射应与 sidebar 排序同步"
    );
    assert_eq!(
        root.sidebar.take_pending_track_port_change(),
        None,
        "pending 取出后应清空"
    );

    let events = lumino_message::events::take_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            lumino_message::events::Event::Window(
                lumino_message::events::window::Event::MidiPortLayoutChanged { max_port: 3 }
            )
        )),
        "max_port 0 -> 3 应发出布局变更事件: {events:?}"
    );
}

/// max_port 不变时不无谓通知输出布局重建
#[test]
fn test_forward_port_change_no_layout_event_when_max_unchanged() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    let mut doc = crate::test_helpers::make_test_document();
    doc.track_ports = vec![0, 5]; // max_port 已为 5
    root.set_midi_document(doc);

    // track 0 端口 0 -> 2：max_port 仍为 5
    root.sidebar.pending_track_port_change = Some((0, 0, 2));
    root.forward_pending_track_port_change();

    assert_eq!(
        root.editor
            .editor_state
            .data
            .document
            .as_ref()
            .expect("文档应存在")
            .track_ports[0],
        2
    );
    let events = lumino_message::events::take_events();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            lumino_message::events::Event::Window(
                lumino_message::events::window::Event::MidiPortLayoutChanged { .. }
            )
        )),
        "max_port 未变化不得触发布局重建: {events:?}"
    );
}

/// 无文档时回滚 sidebar 端口改动并清空 pending（防 UI 与文档静默分叉）
#[test]
fn test_forward_port_change_rolls_back_without_document() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.editor.editor_state.data.document = None;
    root.editor.editor_state.data.modified = false;

    root.sidebar.tracks[1].port = 3;
    root.sidebar.tracks[1].display_label = crate::sidebar::Sidebar::track_label(3, 0);
    root.sidebar.pending_track_port_change = Some((1, 0, 3));

    root.forward_pending_track_port_change();

    let track = root
        .sidebar
        .tracks
        .iter()
        .find(|t| t.id == 1)
        .expect("应找到目标音轨");
    assert_eq!(track.port, 0, "写入失败应回滚端口");
    assert_eq!(track.display_label, "A01", "写入失败应回滚标签");
    assert_eq!(root.sidebar.take_pending_track_port_change(), None);
    assert!(
        !root.editor.editor_state.data.modified,
        "未应用的编辑不得置工程脏"
    );
}
