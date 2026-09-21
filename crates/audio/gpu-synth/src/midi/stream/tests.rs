
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
