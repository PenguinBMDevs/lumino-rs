//! LMPJ 快照契约测试（从 lmpj.rs 拆出，守住文件长度红线）。
//!
//! 子模块通过 `super::*` 访问父模块的类型 `LmpjSnapshot`（对模块外因父模块
//! 私有而不可见，但本测试作为 lmpj 的子模块可见）。

use super::*;
use lumino_midi_loader::{ChunkedList, TrackManager};

fn empty_document() -> MidiDocument {
    MidiDocument {
        notes: Vec::new(),
        tempo_changes: Vec::new(),
        time_signatures: Vec::new(),
        key_signatures: Vec::new(),
        control_events: ChunkedList::new(),
        lyrics: Vec::new(),
        markers: Vec::new(),
        text_events: Vec::new(),
        sys_ex: Vec::new(),
        track_names: Vec::new(),
        total_ticks: 0,
        track_count: 0,
        tracks: TrackManager::new(0),
        division: 480,
        track_ports: Vec::new(),
        track_max_end_ticks: Vec::new(),
    }
}

/// 快照后对原 document 的编辑不得进入本次保存（快照 = Ctrl+S 时点语义）。
///
/// `MidiDocument::clone` 是块级 COW：快照后插入的音符落在原文档的块里，
/// 快照持有的块 Arc 不受影响。本测试锁定这一隔离契约，正是 DEBT-23
/// 把云保存/自动保存改为"主线程快照 + 后台组包"的正确性前提。
#[test]
fn test_snapshot_isolates_later_document_edits() {
    use lumino_midi_loader::NoteEvent;

    let mut live_doc = MidiDocument::empty_with_tracks(1, 480);
    assert!(live_doc.insert_note(0, NoteEvent::new(100, 200, 60, 100, 0)));
    assert_eq!(live_doc.track_notes(0).len(), 1);

    // 主线程快照：COW 共享底层音符块
    let snapshot_doc = live_doc.clone();

    // 快照之后用户在原文档继续编辑
    assert!(live_doc.insert_note(0, NoteEvent::new(300, 400, 62, 90, 0)));
    assert_eq!(live_doc.track_notes(0).len(), 2, "原文档应含新音符");

    // 后台组包：只能看到快照时刻的 1 个音符
    let project = lumino_export::LuminoProject::from_midi_document(&snapshot_doc);
    let loaded_notes: usize = project
        .tracks
        .iter()
        .map(|slot| match slot {
            lumino_export::TrackSlot::Loaded(d) | lumino_export::TrackSlot::Modified(d) => {
                d.note_count as usize
            }
            lumino_export::TrackSlot::Unloaded { .. } => 0,
        })
        .sum();
    assert_eq!(
        loaded_notes, 1,
        "组包必须只见快照时刻的音符，快照后的编辑不得进入本次保存"
    );
}

#[test]
fn test_snapshot_into_project_keeps_metadata() {
    let snapshot = LmpjSnapshot {
        document: empty_document(),
        shapes: Vec::new(),
        author: "Author".into(),
        copyright: "Copyright".into(),
        working_time_seconds: 42.0,
        project_name: "Project".into(),
        use10: true,
        i2m: lumino_core::I2mSettings {
            start_key: 36,
            color_count: 8,
            ..Default::default()
        },
        onion_skin_mode: "previous_track".into(),
    };

    let project = snapshot.into_project();

    assert_eq!(project.metadata.project.name, "Project");
    assert_eq!(project.metadata.project.author, "Author");
    assert_eq!(project.metadata.project.copyright, "Copyright");
    assert_eq!(
        project
            .metadata
            .settings
            .as_ref()
            .and_then(|settings| settings.use10),
        Some(true),
        "启用通道 10 应随工程落盘"
    );
    let i2m = project
        .metadata
        .settings
        .as_ref()
        .and_then(|settings| settings.i2m.clone())
        .expect("图片转 MIDI 参数应随工程落盘");
    assert_eq!((i2m.start_key, i2m.color_count), (36, 8));
    assert_eq!(
        project
            .metadata
            .stats
            .as_ref()
            .and_then(|stats| stats.working_time_seconds),
        Some(42),
        "累计创作时长应取快照时刻"
    );
    // 工程范围记忆：洋葱皮显示范围必须落进 metadata.toml 的 [settings] 段
    assert_eq!(
        project
            .metadata
            .settings
            .as_ref()
            .and_then(|s| s.onion_skin_mode.as_deref()),
        Some("previous_track"),
        "洋葱皮显示范围必须随工程保存（工程范围记忆）"
    );
    // 本任务只接管这一项：其余 [settings] 字段必须保持 None（不越权写别人字段）
    let settings = project
        .metadata
        .settings
        .as_ref()
        .expect("settings 段应存在");
    assert!(settings.auto_scroll_mode.is_none());
    assert!(settings.theme.is_none());
    assert!(settings.enable_256key.is_none());
}
