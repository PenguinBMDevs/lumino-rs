//! 卷帘键盘演奏键色接线：`Editor::playback_key_colors` → `PianoRollView::key_colors`。
//!
//! 回归点：修复前键色只在 Editor 内部计算（`update_playback_key_colors` 照常
//! 产出颜色），但 egui 键盘列完全不读它——设置里「播放时键盘颜色指示」打开后
//! 键盘毫无变化。本测试锁定「数据 → 模型」这一段，像素渲染由
//! `lumino-ui-egui` 的 `piano_roll_skeleton` 测试覆盖。

use crate::root::Root;
use crate::test_helpers::event_queue_lock;
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};

/// 单轨单音符文档（`0..1920`），与 `document_mount` 测试同构。
fn single_note_doc(key: u8) -> MidiDocument {
    MidiDocument {
        notes: vec![ChunkedList::from_sorted(vec![NoteEvent::new(
            0, 1920, key, 100, 0,
        )])],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: 1920,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
    }
}

/// 播放中的键色必须原样进入卷帘骨架模型（键盘列据此绘制洋葱皮）。
#[test]
fn test_piano_roll_view_carries_playback_key_colors() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig {
        playback_key_colors_enabled: true,
        ..Default::default()
    };
    let mut root = Root::new(&ui_config);
    root.editor.editor_state.canvas.size_x = 1280.0;
    root.editor.editor_state.canvas.size_y = 800.0;
    root.set_midi_document(single_note_doc(60));

    root.editor.playback_position = 480.0;
    root.editor.update_playback_key_colors();

    let offset = 60 * 4;
    assert_ne!(
        root.editor.playback_key_colors()[offset + 3],
        0,
        "前置条件：播放位置在音符内时 Editor 必须算出键色"
    );

    let view = root
        .egui_piano_roll_view()
        .expect("默认钢琴卷帘视图应产出骨架模型");
    assert_eq!(
        view.key_colors,
        *root.editor.playback_key_colors(),
        "键盘列模型必须每帧携带播放键色（否则数据算出来也没人画）"
    );
}

/// 开关关闭：Editor 不计算键色，模型也必须保持全零（不残留上一帧颜色）。
#[test]
fn test_piano_roll_view_key_colors_stay_clear_when_disabled() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig::default();
    assert!(
        !ui_config.playback_key_colors_enabled,
        "前置条件：默认配置下「播放时键盘颜色指示」为关闭"
    );
    let mut root = Root::new(&ui_config);
    root.editor.editor_state.canvas.size_x = 1280.0;
    root.editor.editor_state.canvas.size_y = 800.0;
    root.set_midi_document(single_note_doc(60));

    root.editor.playback_position = 480.0;
    root.editor.update_playback_key_colors();

    let view = root
        .egui_piano_roll_view()
        .expect("默认钢琴卷帘视图应产出骨架模型");
    assert_eq!(view.key_colors, [0u8; 1024], "开关关闭时键盘列不得着色");
}
