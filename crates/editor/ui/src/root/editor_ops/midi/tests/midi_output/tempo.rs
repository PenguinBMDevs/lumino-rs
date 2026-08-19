//! tempo changes / BPM 缓存路径 MIDI 输出测试

use crate::editor::note::Note;
use crate::message::Message;
use crate::root::editor_ops::midi::tests::common::{create_mock_output, create_root};
use crate::toolbar;

/// 测试 load_tempo_changes 在无管理器时缓存
#[test]
fn test_tempo_changes_cached_when_no_manager() {
    let mut root = create_root();

    assert!(root.playback.pending_tempo_changes.is_none());

    root.load_tempo_changes(vec![(0, 500000)]); // 120 BPM
    assert!(
        root.playback.pending_tempo_changes.is_some(),
        "无管理器时 tempo changes 应缓存"
    );

    // 播放时应消费缓存的 tempo changes
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));

    assert!(
        root.playback.pending_tempo_changes.is_none(),
        "播放后 tempo changes 应被消费"
    );
    assert!(root.playback.manager.is_some());

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 回归测试：空白工程（无播放管理器）下通过 tempo 编辑器设置 BPM
///
/// 播放管理器是懒创建的，`update_playback_bpm` 在 manager 未初始化时必须像
/// `load_tempo_changes` 一样缓存到 `pending_tempo_changes`，否则设置被静默丢弃，
/// 从头播放时回落默认 120 BPM（"首个 BPM 为默认值"）。
#[test]
fn test_update_playback_bpm_caches_when_no_manager() {
    let mut root = create_root();

    assert!(root.playback.manager.is_none());
    assert!(root.playback.pending_tempo_changes.is_none());

    // 模拟空白工程下用户设置 BPM=140（Conductor 轨 tempo 编辑）
    root.editor.editor_state.data.tempo_points = vec![crate::editor::editor_state::TempoPoint {
        tick: 0.0,
        bpm: 140.0,
    }];
    root.update_playback_bpm();

    assert!(
        root.playback.pending_tempo_changes.is_some(),
        "无播放管理器时 update_playback_bpm 应缓存 tempo changes"
    );

    // 播放应消费缓存并应用 BPM
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());
    assert!(
        root.playback.pending_tempo_changes.is_none(),
        "播放后 tempo changes 应被消费"
    );

    // 播放线程异步处理 SetTempoChanges 命令，轮询等待 timeline 生效
    if let Some(manager) = &root.playback.manager {
        let mut got = 0.0f64;
        for _ in 0..100 {
            let bpm = manager.current_bpm();
            if (bpm - 140.0).abs() < 0.5 {
                got = bpm;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            (got - 140.0).abs() < 0.5,
            "播放管理器应应用用户设置的 BPM 140，实际 {got}"
        );
    }

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 回归测试：先工程设置 BPM，再在 tempo 编辑器继续编辑（多个控制点），
/// 缓存必须保持最新——不允许"只识别第一个 BPM，后续编辑被忽略"。
#[test]
fn test_update_playback_bpm_overwrites_stale_cache() {
    let mut root = create_root();

    // 工程设置路径：manager 不存在 → 缓存 BPM=140
    root.editor.editor_state.data.tempo_points = vec![crate::editor::editor_state::TempoPoint {
        tick: 0.0,
        bpm: 140.0,
    }];
    root.update_playback_bpm();
    assert!(root.playback.pending_tempo_changes.is_some());

    // 用户继续拖拽第一个控制点到 160、并追加第二个控制点 480tick/100BPM
    // （同样在 manager 创建前，全部走缓存路径）
    root.editor.editor_state.data.tempo_points = vec![
        crate::editor::editor_state::TempoPoint {
            tick: 0.0,
            bpm: 160.0,
        },
        crate::editor::editor_state::TempoPoint {
            tick: 480.0,
            bpm: 100.0,
        },
    ];
    root.update_playback_bpm();

    // 播放 → 缓存被消费，timeline 使用最新全量 tempo_points
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.pending_tempo_changes.is_none());

    if let Some(manager) = &root.playback.manager {
        let mut got = 0.0f64;
        for _ in 0..100 {
            let bpm = manager.current_bpm();
            if (bpm - 160.0).abs() < 0.5 {
                got = bpm;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            (got - 160.0).abs() < 0.5,
            "缓存应被最新编辑覆盖（首个点 160 BPM），实际 {got}"
        );
    }

    root.update(Message::Toolbar(toolbar::Event::Stop));
}
