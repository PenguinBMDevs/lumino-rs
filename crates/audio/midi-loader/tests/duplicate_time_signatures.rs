//! #146 前置契约：加载器必须**全量保留**同 tick 的重复拍号。
//!
//! 多轨 MIDI 常见两轨各写一个 tick 0 的 4/4。加载器为保住 EXP-005 的逐事件
//! 往返等价而刻意不去重；标尺显示层（`ui-editor/grid/ruler.rs`）依赖该契约
//! 自行去重。若本测试变红，说明加载口径变了，必须同步复核标尺/网格/导出。

use lumino_midi_loader::loader::load_parsed_midi_from_bytes;

fn track(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// format 1 / 2 轨 / division 960；两轨各写一个 tick 0 的 4/4。
fn two_track_midi_with_duplicate_time_signature() -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6u32.to_be_bytes());
    file.extend_from_slice(&1u16.to_be_bytes()); // format 1
    file.extend_from_slice(&2u16.to_be_bytes()); // 2 tracks
    file.extend_from_slice(&960u16.to_be_bytes()); // division 960

    for _ in 0..2 {
        file.extend(track(&[
            0x00, 0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08, // 4/4 @ tick 0
            0x00, 0xFF, 0x2F, 0x00, // end of track
        ]));
    }
    file
}

#[tokio::test]
async fn duplicate_time_signatures_at_same_tick_are_preserved() {
    let parsed =
        load_parsed_midi_from_bytes(two_track_midi_with_duplicate_time_signature(), 2, 0, None)
            .await
            .expect("MIDI 加载失败");
    let doc = parsed.document.expect("ParsedMidi 应携带 document");

    assert_eq!(
        doc.time_signatures,
        vec![(0, 4, 4), (0, 4, 4)],
        "同 tick 重复拍号必须全量保留（EXP-005 口径；标尺显示层负责去重）"
    );
}
