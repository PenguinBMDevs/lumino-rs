//! 对拍：同一条 `.mid`，两条事件提取链必须产出**相同的事件多重集**
//!
//! 审计 §6.2 记录：仓库自述「文件模式与内存模式逐字节一致」**当前只有人工观测**
//! （`crates/audio/gpu-synth/src/synth/engine/render.rs:171-178`），没有自动化
//! 保障。两条链读的是同一份磁盘数据，但实现完全不同：
//!
//! | 模式 | 事件来源 | 解析器 |
//! |---|---|---|
//! | 文件回退 | `StreamingMidiPlayer`（mmap 零拷贝逐事件） | `MidiLoader::MmapSmf` |
//! | 内存（首选） | `MidiDocument`（`from_notes_bytes`） → `MidiDocEventStream` | `lumino-midly` → 文档构建器 |
//!
//! 本测试用**自足生成**的 SMF（不依赖任何外部夹具）把「两条链等价」从人工观测
//! 升级为回归断言：音符起音、音符结束、CC、PC、PB 五类事件按
//! `(tick, port, channel, param…)` 计数必须逐项相同。
//!
//! 若此测试失败，说明「用户在编辑器里打开该文件后导出音频」与「不打开文件直接
//! 导出音频」会得到不同结果——那正是需要修的分叉，而不是测试的问题。

use std::collections::BTreeMap;

use lumino_midi_loader::{MidiDocument, StreamingMidiPlayer};
use midly::{MidiMessage, TrackEventKind};

use super::MidiDocEventStream;

/// 事件键：`(tick, port, channel, param1, param2)`
type EventKey = (u64, u8, u8, u8, u16);

/// 一条链提取出的事件多重集
#[derive(Debug, Default, PartialEq, Eq)]
struct EventSet {
    note_on: BTreeMap<EventKey, usize>,
    note_off: BTreeMap<(u64, u8, u8, u8), usize>,
    cc: BTreeMap<EventKey, usize>,
    pc: BTreeMap<EventKey, usize>,
    pb: BTreeMap<EventKey, usize>,
}

/// 文档链（内存模式）：`MidiDocument` → `MidiDocEventStream`
fn collect_from_document(bytes: &[u8]) -> EventSet {
    let (doc, _, _) = MidiDocument::from_notes_bytes(bytes, None).expect("文档应可解析");
    let mut stream = MidiDocEventStream::new(&doc);
    let mut set = EventSet::default();

    while let Some(ev) = stream.next_event() {
        let key = (
            u64::from(ev.tick),
            ev.port,
            ev.channel,
            ev.param1,
            ev.param2,
        );
        match ev.kind {
            0 => *set.note_on.entry(key).or_default() += 1,
            1 => {
                *set.note_off
                    .entry((u64::from(ev.tick), ev.port, ev.channel, ev.param1))
                    .or_default() += 1
            }
            2 => *set.cc.entry(key).or_default() += 1,
            3 => *set.pc.entry(key).or_default() += 1,
            4 => *set.pb.entry(key).or_default() += 1,
            _ => {}
        }
    }
    set
}

/// 文件链（文件回退模式）：`StreamingMidiPlayer` 逐事件
fn collect_from_streaming(bytes: &[u8]) -> EventSet {
    let mut player = StreamingMidiPlayer::from_bytes(bytes).expect("流式解析应成功");
    let ports = player.track_ports().to_vec();
    let mut set = EventSet::default();

    while let Some((tick, track_idx, kind)) = player.next_event() {
        let port = ports.get(track_idx).copied().unwrap_or(0);
        let TrackEventKind::Midi { channel, message } = kind else {
            continue; // meta（tempo/拍号/轨名…）不参与本次对拍
        };
        let ch = u8::from(channel);
        match message {
            MidiMessage::NoteOn { key, vel } => {
                // 本 fork：`key` 已是 u8，`vel` 是 u7
                let vel = u8::from(vel);
                if vel == 0 {
                    // MIDI 约定：vel=0 的 NoteOn 等价 NoteOff
                    *set.note_off.entry((tick, port, ch, key)).or_default() += 1;
                } else {
                    *set.note_on
                        .entry((tick, port, ch, key, u16::from(vel)))
                        .or_default() += 1;
                }
            }
            MidiMessage::NoteOff { key, .. } => {
                *set.note_off.entry((tick, port, ch, key)).or_default() += 1;
            }
            MidiMessage::Controller { controller, value } => {
                *set.cc
                    .entry((
                        tick,
                        port,
                        ch,
                        u8::from(controller),
                        u16::from(u8::from(value)),
                    ))
                    .or_default() += 1;
            }
            MidiMessage::ProgramChange { program } => {
                *set.pc
                    .entry((tick, port, ch, u8::from(program), 0))
                    .or_default() += 1;
            }
            MidiMessage::PitchBend { bend } => {
                // `PitchBend` 是 newtype（内层 u14 = 原始 14 位）；`as_int()` 返回
                // 带符号居中值，与文档链的原始值口径不同，故取内层 u14。
                *set.pb
                    .entry((tick, port, ch, 0, bend.0.as_int()))
                    .or_default() += 1;
            }
            _ => {}
        }
    }
    set
}

// ── 自足 SMF 生成器（不依赖外部夹具） ─────────────────────────────────────

/// MIDI 变长量（VLQ）
fn vlq(mut value: u32) -> Vec<u8> {
    let mut buf = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        buf.insert(0, ((value & 0x7F) as u8) | 0x80);
        value >>= 7;
    }
    buf
}

fn push_track(out: &mut Vec<u8>, track: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend_from_slice(track);
}

/// 生成覆盖五类事件 + 端口元事件的测试 SMF（Format 1 / 480 PPQ / 3 轨）
///
/// - 轨 0：tempo 120@0 → 90@960
/// - 轨 1：`FF 21 01`（端口 1）+ NoteOn/CC/NoteOff/PC/PB
/// - 轨 2：通道 1 的音符
fn generated_midi() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // format 1
    out.extend_from_slice(&3u16.to_be_bytes()); // 3 tracks
    out.extend_from_slice(&480u16.to_be_bytes()); // 480 PPQ

    // 轨 0：速度变化
    let mut t0 = vlq(0);
    t0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 120 BPM
    t0.extend_from_slice(&vlq(960));
    t0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x0A, 0x2C, 0x2A]); // 90 BPM
    t0.extend_from_slice(&vlq(0));
    t0.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    push_track(&mut out, &t0);

    // 轨 1：端口元事件 + 五类事件
    // 注意 `FF 21 <len=1> <port>` 是 **4 字节**：漏掉数据字节会让解析器把后续
    // 字节当成端口数据，整轨错位（本测试首版即踩过，被自己抓到）
    let mut t1 = vlq(0);
    t1.extend_from_slice(&[0xFF, 0x21, 0x01, 0x01]); // MIDI Port = 1
    t1.extend_from_slice(&vlq(0));
    t1.extend_from_slice(&[0x90, 60, 100]); // NoteOn ch0 key60 vel100 @0
    t1.extend_from_slice(&vlq(240));
    t1.extend_from_slice(&[0xB0, 7, 100]); // CC7 = 100 @240
    t1.extend_from_slice(&vlq(240));
    t1.extend_from_slice(&[0x80, 60, 64]); // NoteOff key60 release 64 @480
    t1.extend_from_slice(&vlq(0));
    t1.extend_from_slice(&[0xC0, 5]); // PC 5 @480
    t1.extend_from_slice(&vlq(0));
    t1.extend_from_slice(&[0xE0, 0x00, 0x50]); // PB = 0x50<<7 = 10240 @480
    t1.extend_from_slice(&vlq(480));
    t1.extend_from_slice(&[0x90, 62, 90]); // NoteOn key62 vel90 @960
    t1.extend_from_slice(&vlq(240));
    t1.extend_from_slice(&[0x80, 62, 32]); // NoteOff @1200
    t1.extend_from_slice(&vlq(0));
    t1.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    push_track(&mut out, &t1);

    // 轨 2：通道 1 的音符（无端口元事件 → 端口 0）
    let mut t2 = vlq(240);
    t2.extend_from_slice(&[0x91, 64, 80]); // NoteOn ch1 key64 vel80 @240
    t2.extend_from_slice(&vlq(480));
    t2.extend_from_slice(&[0x81, 64, 64]); // NoteOff @720
    t2.extend_from_slice(&vlq(0));
    t2.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    push_track(&mut out, &t2);

    out
}

#[test]
fn document_and_streaming_chains_agree_on_generated_midi() {
    let bytes = generated_midi();
    let from_doc = collect_from_document(&bytes);
    let from_file = collect_from_streaming(&bytes);

    // 先确认样本本身有内容（避免"两边都空"的假绿）
    assert!(
        from_file.note_on.len() >= 3 && from_file.cc.len() == 1 && from_file.pb.len() == 1,
        "生成器应产出 ≥3 个音符起音 + CC + PB，实际: {from_file:?}"
    );

    assert_eq!(
        from_doc.note_on, from_file.note_on,
        "音符起音 (tick, port, channel, key, vel) 多重集必须一致"
    );
    assert_eq!(
        from_doc.note_off, from_file.note_off,
        "音符结束 (tick, port, channel, key) 多重集必须一致"
    );
    assert_eq!(from_doc.cc, from_file.cc, "CC 多重集必须一致");
    assert_eq!(from_doc.pc, from_file.pc, "PC 多重集必须一致");
    assert_eq!(from_doc.pb, from_file.pb, "PB 多重集必须一致");
}

#[test]
fn document_chain_reports_port_meta_identically() {
    let bytes = generated_midi();
    let from_doc = collect_from_document(&bytes);

    // 轨 1 声明了 FF 21 = 1：两条链都必须把该轨事件标成端口 1
    let ports: std::collections::BTreeSet<u8> = from_doc.note_on.keys().map(|k| k.1).collect();
    assert_eq!(
        ports,
        std::collections::BTreeSet::from([0u8, 1u8]),
        "文档链应还原端口元事件（轨1=端口1，轨2=端口0），实际 {ports:?}"
    );
}
