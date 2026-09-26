use super::*;
use lumino_midi_loader::{MidiDocument, NoteEvent, TrackManager};

fn make_test_doc() -> MidiDocument {
    let mut track0 = vec![
        NoteEvent::new(480, 960, 64, 100, 0), // E4
        NoteEvent::new(0, 480, 60, 100, 0),   // C4
    ];
    track0.sort_unstable_by_key(|n| n.start_tick);

    let track1 = vec![NoteEvent::new(0, 1920, 67, 100, 1)]; // G4

    MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(track0),
            lumino_midi_loader::ChunkedList::from_sorted(track1),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 1".into()), Some("Track 2".into())],
        total_ticks: 1920,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    }
}

#[test]
fn test_playback_key_colors_basic() {
    let doc = make_test_doc();
    let mut state = PlaybackKeyColorState::default();
    let mut colors = [0u8; KEY_COLOR_BYTES];

    update_playback_key_colors(&doc, 240, &mut state, &mut colors);

    // C4 (key=60) 与 G4 (key=67) 在 tick 240 活跃
    assert_ne!(colors[60 * 4 + 3], 0, "C4 应被着色");
    assert_ne!(colors[67 * 4 + 3], 0, "G4 应被着色");
    // E4 (key=64) 尚未开始
    assert_eq!(colors[64 * 4 + 3], 0, "E4 不应被着色");
}

#[test]
fn test_playback_key_colors_at_boundary() {
    let doc = make_test_doc();
    let mut state = PlaybackKeyColorState::default();
    let mut colors = [0u8; KEY_COLOR_BYTES];

    // tick=480：C4 刚结束，E4 刚开始，G4 仍活跃
    update_playback_key_colors(&doc, 480, &mut state, &mut colors);

    assert_eq!(colors[60 * 4 + 3], 0, "C4 已结束");
    assert_ne!(colors[64 * 4 + 3], 0, "E4 应活跃");
    assert_ne!(colors[67 * 4 + 3], 0, "G4 仍活跃");
}

#[test]
fn test_playback_key_colors_incremental_consistency() {
    let doc = make_test_doc();
    let mut state = PlaybackKeyColorState::default();
    let mut colors = [0u8; KEY_COLOR_BYTES];

    // 首次全量扫描
    update_playback_key_colors(&doc, 240, &mut state, &mut colors);
    assert_ne!(colors[60 * 4 + 3], 0);
    assert_ne!(colors[67 * 4 + 3], 0);

    // 增量前进到 480
    update_playback_key_colors(&doc, 480, &mut state, &mut colors);
    assert_eq!(colors[60 * 4 + 3], 0);
    assert_ne!(colors[64 * 4 + 3], 0);
    assert_ne!(colors[67 * 4 + 3], 0);

    // 回退触发全量重建，结果应与全量一致
    update_playback_key_colors(&doc, 120, &mut state, &mut colors);
    assert_ne!(colors[60 * 4 + 3], 0);
    assert_eq!(colors[64 * 4 + 3], 0);
    assert_ne!(colors[67 * 4 + 3], 0);
}

#[test]
fn test_playback_key_colors_no_overflow() {
    // 验证 key 索引在 127 以上时不会越界写入
    let doc = MidiDocument {
        notes: vec![lumino_midi_loader::ChunkedList::from_sorted(vec![
            NoteEvent::new(0, 100, 200, 100, 0),
        ])],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T".into())],
        total_ticks: 100,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    };
    let mut state = PlaybackKeyColorState::default();
    let mut colors = [0u8; KEY_COLOR_BYTES];
    update_playback_key_colors(&doc, 50, &mut state, &mut colors);
    // 虽然 key=200 超出 128，但 offset=800，仍在 1024 缓冲区内
    assert_ne!(colors[200 * 4 + 3], 0);
    // 合成函数只读取 0..127，不会因此崩溃
}

#[test]
fn test_composite_keyboard_colors_blend() {
    const WIDTH: u32 = 60;
    // 使用 zoom_y=2，确保最高键（key=127）至少有两行像素，
    // 可避开白键底部的 1px 边框行（基础色为浅灰而非纯白）。
    const HEIGHT: u32 = 30 + 256; // ruler 30 + 128 键 × 2
    let (kb_pixels, kb_w, kb_h) = generate_keyboard_texture(WIDTH, HEIGHT, 128);
    let mut frame = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    let mut key_colors = [0u8; KEY_COLOR_BYTES];
    // 将最高键（key=127，顶部 py=0）着为纯红
    key_colors[127 * 4] = 255;
    key_colors[127 * 4 + 3] = 255;

    composite_keyboard(
        &mut frame,
        WIDTH,
        HEIGHT,
        &kb_pixels,
        kb_w,
        kb_h,
        &key_colors,
    );

    // ruler 下方第一行（key=127 的非边框行）最左侧像素应被混合为红色
    let idx = (30 * WIDTH as usize) * 4;
    // 白键基础色为 (255,255,255)，叠加 (255,0,0) × 0.6 → (102,102,255)
    assert_eq!(frame[idx], 102, "B 通道应被混合");
    assert_eq!(frame[idx + 1], 102, "G 通道应被混合");
    assert_eq!(frame[idx + 2], 255, "R 通道应保持 255");
    assert_eq!(frame[idx + 3], 255, "Alpha 应为不透明");
}
