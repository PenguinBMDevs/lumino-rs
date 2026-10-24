//! 音轨静音/独奏写回 document 测试（防「UI 能切、保存丢失」）
//!
//! `.lmpj` 保存链路（`LuminoProject::from_midi_document`）从 `document.tracks`
//! 读取每轨 `visibility`/`solo` 落盘。修复前 UI 的静音/独奏只改
//! `sidebar.tracks[].is_muted/is_soloed`，两侧零同步 —— 用户切了静音、保存、
//! 重开，拿到的是加载时的旧状态。

use crate::Message;
use crate::root::Root;
use crate::test_helpers::event_queue_lock;
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::{ChunkedList, MidiDocument, TrackManager, TrackVisibility};

/// 构造 n 轨空文档；`hidden` 指定一轨预置 `Hidden`（模拟 `.lmpj` 元数据里的隐藏轨）
fn doc_with_tracks(n: u16, hidden: Option<u16>) -> MidiDocument {
    let mut doc = MidiDocument {
        notes: (0..n).map(|_| ChunkedList::new()).collect(),
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None; n as usize],
        total_ticks: 480,
        track_count: n,
        tracks: TrackManager::new(n),
        division: 480,
        track_ports: vec![0; n as usize],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(n as usize),
        track_max_note_lens: MidiDocument::new_track_max_note_lens_unknown(n as usize),
    };
    if let Some(id) = hidden {
        doc.tracks.set_visibility(id, TrackVisibility::Hidden);
    }
    doc
}

fn root_with_doc(n: u16, hidden: Option<u16>) -> Root {
    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.set_midi_document(doc_with_tracks(n, hidden));
    root.editor.editor_state.data.modified = false;
    root
}

/// 取音轨可见性（测试辅助，避免重复的 as_ref().expect() 链）
fn visibility_of(root: &Root, track_id: u16) -> TrackVisibility {
    root.editor
        .editor_state
        .data
        .document
        .as_ref()
        .expect("文档应存在")
        .tracks
        .get(track_id)
        .expect("音轨应存在")
        .visibility
}

fn solo_of(root: &Root, track_id: u16) -> bool {
    root.editor
        .editor_state
        .data
        .document
        .as_ref()
        .expect("文档应存在")
        .tracks
        .get(track_id)
        .expect("音轨应存在")
        .solo
}

/// 静音切换必须写回 `doc.tracks` 并置脏（否则保存丢失）
#[test]
fn test_mute_toggle_writes_to_document() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let mut root = root_with_doc(2, None);

    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackMuteToggled(
            1
        )))
    );
    assert_eq!(
        visibility_of(&root, 1),
        TrackVisibility::Muted,
        "静音必须写回保存源（.lmpj 读 doc.tracks）"
    );
    assert_eq!(
        visibility_of(&root, 0),
        TrackVisibility::Visible,
        "不得波及其它音轨"
    );
    assert!(
        root.editor.editor_state.data.modified,
        "写回保存源应把工程置脏"
    );

    // 再切一次 → 回到可见
    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackMuteToggled(
            1
        )))
    );
    assert_eq!(
        visibility_of(&root, 1),
        TrackVisibility::Visible,
        "取消静音应回写 Visible"
    );
}

/// 独奏切换必须写回 `doc.tracks.solo`
#[test]
fn test_solo_toggle_writes_to_document() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let mut root = root_with_doc(2, None);

    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackSoloToggled(
            1
        )))
    );
    assert!(solo_of(&root, 1), "独奏必须写回保存源");
    assert!(!solo_of(&root, 0), "不得波及其它音轨");

    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackSoloToggled(
            1
        )))
    );
    assert!(!solo_of(&root, 1), "取消独奏应回写 false");
}

/// `Hidden` 是 UI 未建模的持久化状态：静音切换不得把它降级
#[test]
fn test_mute_toggle_never_downgrades_hidden() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let mut root = root_with_doc(2, Some(0));

    // 直接切隐藏轨自己的静音
    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackMuteToggled(
            0
        )))
    );
    assert_eq!(
        visibility_of(&root, 0),
        TrackVisibility::Hidden,
        "Hidden 不得被静音开关降级为 Visible/Muted"
    );

    // 切别的轨，隐藏轨同样不受影响
    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackMuteToggled(
            1
        )))
    );
    assert_eq!(
        visibility_of(&root, 0),
        TrackVisibility::Hidden,
        "同批切换不得把 Hidden 轨抹平"
    );
    assert_eq!(visibility_of(&root, 1), TrackVisibility::Muted);
}

/// 无文档时不得 panic：UI/播放仍按 sidebar 生效（无处可写）
#[test]
fn test_mute_toggle_without_document_keeps_ui_state() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.editor.editor_state.data.document = None;

    assert!(
        root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackMuteToggled(
            0
        )))
    );
    assert!(
        root.sidebar.tracks[0].is_muted,
        "无文档时 UI 静音仍应生效（不回滚）"
    );
}
