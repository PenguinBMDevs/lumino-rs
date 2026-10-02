//! REND-002 CPU 导出多端口回归（仓库内自动 fixture）。
//!
//! 覆盖：FF 21 端口提取 → `max_port` → 合成格式（Custom 通道数 / 打击乐初始化）。
//! 不依赖外部素材与音色库；Night Voyager 7 端口素材的听感/事件级终验另行人工执行。

use lumino_export::audio::config::AudioRenderConfig;
use lumino_midi_loader::MidiDocument;
use xsynth_core::channel_group::SynthFormat;

/// MIDI 变长数量（VLQ）编码。
fn vlq(mut n: u32) -> Vec<u8> {
    let mut bytes = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push(((n & 0x7F) as u8) | 0x80);
        n >>= 7;
    }
    bytes.reverse();
    bytes
}

/// 将事件字节序列封装为 MTrk 块并追加到 `out`。
fn push_track(out: &mut Vec<u8>, events: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(events.len() as u32).to_be_bytes());
    out.extend_from_slice(events);
}

/// 构造 SMF：`ports[i] = Some(p)` 时轨 i 写入 FF 21 MidiPort=p；两轨都是
/// ch0/key60（跨端口同 (channel,key)，正是端口合流问题的触发形态）。
fn build_midi(ports: &[Option<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // format 1
    out.extend_from_slice(&(ports.len() as u16).to_be_bytes());
    out.extend_from_slice(&480u16.to_be_bytes());

    for port in ports {
        let mut track = vlq(0);
        if let Some(p) = port {
            track.extend_from_slice(&[0xFF, 0x21, 0x01, *p]); // FF 21 MidiPort
        }
        track.extend_from_slice(&[0x90, 60, 100]); // NoteOn ch0 key60
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0x80, 60, 64]); // NoteOff
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]); // End of Track
        push_track(&mut out, &track);
    }
    out
}

/// 双端口 fixture：轨 0 端口 0、轨 1 端口 1 → Custom{32} + 每端口 ch9 打击乐。
#[test]
fn multi_port_document_enables_custom_channels() {
    let bytes = build_midi(&[Some(0), Some(1)]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("双端口 SMF 应可解析");

    assert_eq!(doc.track_port(0), 0);
    assert_eq!(doc.track_port(1), 1, "轨 1 应提取 FF 21 端口");
    assert_eq!(doc.max_port(), 1);

    let config = AudioRenderConfig {
        midi_max_port: doc.max_port(),
        ..Default::default()
    };
    assert_eq!(
        config.synth_format(),
        SynthFormat::Custom { channels: 32 },
        "双端口应开通 32 个全局通道"
    );
    assert_eq!(
        config.percussion_channels(),
        vec![9, 25],
        "两个端口的 ch9 都必须显式初始化打击乐"
    );
}

/// 单端口基线（无 FF 21）：`max_port=0` → `SynthFormat::Midi`，零行为变化。
#[test]
fn single_port_document_keeps_midi_baseline() {
    let bytes = build_midi(&[None]);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("单端口 SMF 应可解析");

    assert_eq!(doc.max_port(), 0, "无 FF 21 时最大端口应为 0");

    let config = AudioRenderConfig {
        midi_max_port: doc.max_port(),
        ..Default::default()
    };
    assert_eq!(config.synth_format(), SynthFormat::Midi);
    assert!(
        config.percussion_channels().is_empty(),
        "单端口不应显式下发打击乐（Midi 自动开启）"
    );
}
