use super::*;
/// 构造最小合法 SMF（1 轨：tempo + note on/off + end），避免依赖 gitignored 大文件。
fn minimal_smf() -> Vec<u8> {
    let mut v = Vec::new();
    // MThd: format 0, 1 track, division 480
    v.extend_from_slice(b"MThd");
    v.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0]);
    // Track data
    let mut track = Vec::new();
    // delta 0, tempo 500000 (07 A1 20)
    track.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
    // delta 0, note-on ch0 key60 vel100
    track.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]);
    // delta 480 (83 60), note-off ch0 key60 vel64
    track.extend_from_slice(&[0x83, 0x60, 0x80, 0x3C, 0x40]);
    // delta 0, end of track
    track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    v.extend_from_slice(b"MTrk");
    v.extend_from_slice(&(track.len() as u32).to_be_bytes());
    v.extend_from_slice(&track);
    v
}
#[test]
fn stream_roundtrip_small() {
    let raw = minimal_smf();
    let midi = MidiStream::parse(&raw, 64_000).expect("最小 SMF 应可解析");
    assert!(midi.end_sample() > 0);
    assert!(!midi.is_exhausted());
    let mut s = midi;
    let mut last = 0u32;
    let mut c = 0;
    while let Some(ev) = s.next_event() {
        assert!(ev.sample >= last);
        last = ev.sample;
        c += 1;
    }
    assert!(c > 0);
    assert!(s.is_exhausted());
}

/// 双轨各写 FF 21 端口：流式事件必须映射为全局通道（轨 1 → 16）。
#[test]
fn stream_maps_ports_to_global_channels() {
    let mut v = Vec::new();
    v.extend_from_slice(b"MThd");
    v.extend_from_slice(&[0, 0, 0, 6, 0, 1, 0, 2, 0x01, 0xE0]); // format 1, 2 tracks, 480
    for p in [0u8, 1] {
        let mut track = Vec::new();
        track.extend_from_slice(&[0x00, 0xFF, 0x21, 0x01, p]);
        track.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]);
        track.extend_from_slice(&[0x83, 0x60, 0x80, 0x3C, 0x40]);
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        v.extend_from_slice(b"MTrk");
        v.extend_from_slice(&(track.len() as u32).to_be_bytes());
        v.extend_from_slice(&track);
    }
    let mut midi = MidiStream::parse(&v, 64_000).expect("双端口 SMF 应可解析");
    assert_eq!(midi.max_port(), 1);
    let mut channels = Vec::new();
    while let Some(ev) = midi.next_event() {
        if ev.kind() == crate::midi::kind::NOTE_ON {
            channels.push(ev.channel());
        }
    }
    channels.sort_unstable();
    assert_eq!(channels, vec![0, 16], "轨 1 的 ch0 应映射到全局 16");
}
