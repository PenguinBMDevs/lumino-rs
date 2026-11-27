//! REND-002 修复回归：文档真正切换时打击乐模态跟踪必须清零。
//!
//! 背景：`PlaybackEngine::set_document` 同时服务「编辑后快照更新」，那条路径
//! 必须保留运行时 Bank Select 推导出的模态状态，因此清零不能放进
//! `set_document`——由真正的切换点（UI 侧 `Root::set_midi_document`）显式发
//! `PlaybackManager::reset_percussion_tracking`。

use super::*;

/// 构造双端口文档（`track_ports = [0, 1]` → 通道空间 32）。
fn two_port_doc() -> Arc<MidiDocument> {
    Arc::new(MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::new(),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None, None],
        total_ticks: 0,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 1],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(2),
    })
}

/// 上一文档在 ch9 留下「旋律 + 已有 Bank Select 证据」，重置后必须回到
/// 「默认打击乐 + 无证据」。
///
/// 回归意义：`msb_seen` 跨文档泄漏会让新文档只发 CC32（无 CC0）时被误判为
/// 需要切换模态——`observe_cc` 在 `msb_seen == false` 时明确不参与判定。
#[test]
fn test_reset_percussion_tracking_clears_cross_document_leak() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    // 造出「上一文档残留」：CC0=0（非鼓 bank）把默认 ch9 切成旋律
    assert_eq!(
        engine.percussion.observe_cc(9, 0, 0),
        Some(false),
        "CC0=0 应把默认 ch9 打击乐切成旋律"
    );
    assert!(!engine.percussion.is_percussion(9));
    assert!(engine.percussion.has_evidence(9));

    engine.reset_percussion_tracking();

    assert!(
        engine.percussion.is_percussion(9),
        "重置后 ch9 必须回到默认打击乐模态"
    );
    assert!(
        !engine.percussion.has_evidence(9),
        "重置后 Bank Select 证据必须清零（泄漏会让 CC32 被误判）"
    );
    assert_eq!(engine.percussion.channels(), 16, "无文档时按 16 通道重建");
}

/// 重置必须沿用**文档的通道空间**（多端口布局下不能塌回 16 通道）。
#[test]
fn test_reset_percussion_tracking_keeps_document_channel_space() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);

    engine.set_document(two_port_doc(), 0);
    assert_eq!(
        engine.percussion.channels(),
        32,
        "双端口文档应给出 32 个全局通道"
    );

    // 端口 1 的 ch9（全局通道 25）被上一文档判为旋律
    assert_eq!(engine.percussion.observe_cc(25, 0, 0), Some(false));
    assert!(!engine.percussion.is_percussion(25));

    engine.reset_percussion_tracking();

    assert_eq!(
        engine.percussion.channels(),
        32,
        "重置必须沿用文档通道空间，不得塌回 16"
    );
    assert!(
        engine.percussion.is_percussion(9),
        "端口 0 的 ch9 应回到默认打击乐"
    );
    assert!(
        engine.percussion.is_percussion(25),
        "端口 1 的 ch9 应回到默认打击乐"
    );
    assert!(!engine.percussion.has_evidence(25), "端口 1 证据应清零");
}
