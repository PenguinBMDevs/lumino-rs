//! Root 侧音轨通道编辑写回测试（批量改写音符/控制事件，防"只改显示"）

use crate::root::Root;
use crate::test_helpers::event_queue_lock;
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};
use lumino_midi_model::PackedControlEvent;

/// 构造两轨文档：track 1 含一颗 ch0 音符 + 一条 ch0 ProgramChange。
fn doc_with_channel_content() -> MidiDocument {
    let notes = vec![NoteEvent::new(0, 480, 60, 100, 0)];
    MidiDocument {
        notes: vec![ChunkedList::new(), ChunkedList::from_sorted(notes)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::from_sorted(vec![PackedControlEvent::program_change(
            0, 1, 0, 5,
        )]),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 480,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(2),
    }
}

/// 通道编辑批量改写音符与控制事件，置脏并同步视觉顺序
#[test]
fn test_forward_channel_change_rewrites_document() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events(); // 清掉并行测试残留事件

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.set_midi_document(doc_with_channel_content());
    root.editor.editor_state.data.modified = false;

    // 模拟 sidebar 已即时更新：track 1 通道 0 -> 3（pending 携带旧值）
    root.sidebar.tracks[1].channel = 3;
    root.sidebar.tracks[1].display_label = crate::sidebar::Sidebar::track_label(0, 3);
    root.sidebar.pending_track_channel_change = Some((1, 0, 3));

    root.forward_pending_track_channel_change();

    let doc = root
        .editor
        .editor_state
        .data
        .document
        .as_ref()
        .expect("文档应存在");
    let note = doc.notes[1].iter().next().expect("track 1 应有音符");
    assert_eq!(note.channel, 3, "音符通道应被改写");
    let control = doc.control_events.iter().next().expect("应有控制事件");
    assert_eq!(control.channel, 3, "控制事件通道应被改写");
    assert_eq!(doc.track_channel(1), 3, "推导通道应随内容更新");
    assert!(root.editor.editor_state.data.modified, "通道编辑应置工程脏");
    assert_eq!(
        root.editor.editor_state.data.track_visual_order,
        root.sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        "视觉顺序映射应与 sidebar 排序同步"
    );
    assert_eq!(root.sidebar.take_pending_track_channel_change(), None);
}

/// 无文档时回滚 sidebar 通道改动并清空 pending（防 UI 与文档静默分叉）
#[test]
fn test_forward_channel_change_rolls_back_without_document() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.editor.editor_state.data.document = None;
    root.editor.editor_state.data.modified = false;

    root.sidebar.tracks[1].channel = 3;
    root.sidebar.tracks[1].display_label = crate::sidebar::Sidebar::track_label(0, 3);
    root.sidebar.pending_track_channel_change = Some((1, 0, 3));

    root.forward_pending_track_channel_change();

    let track = root
        .sidebar
        .tracks
        .iter()
        .find(|t| t.id == 1)
        .expect("应找到目标音轨");
    assert_eq!(track.channel, 0, "写入失败应回滚通道");
    assert_eq!(track.display_label, "A01", "写入失败应回滚标签");
    assert_eq!(root.sidebar.take_pending_track_channel_change(), None);
    assert!(
        !root.editor.editor_state.data.modified,
        "未应用的编辑不得置工程脏"
    );
}
