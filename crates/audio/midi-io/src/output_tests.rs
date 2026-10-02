//! `OutputConnection` 默认实现测试（自 `lib.rs` 拆出，保持单文件 < 400 行）

use super::*;
use std::sync::{Arc, Mutex};

/// 记录原始 MIDI 字节的测试连接（只实现 `send_raw`，其余走默认实现）。
struct RawRecorder {
    sent: Arc<Mutex<Vec<[u8; 3]>>>,
}

impl OutputConnection for RawRecorder {
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        self.sent.lock().expect("锁未 poison").push(data);
        Ok(())
    }
    fn close(self: Box<Self>) {}
}

/// REND-002：显式加入播放能力扩展（测试用默认实现）。
impl PlaybackOutput for RawRecorder {}

/// REND-002 决策 a：默认实现把 u16 全局通道折叠到低 4 位，
/// 外部 MIDI 设备行为与历史一致（端口 B ch9 折叠到线通道 9）。
#[test]
fn default_output_folds_global_channel_to_low_nibble() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut conn = RawRecorder {
        sent: Arc::clone(&sent),
    };
    conn.note_on(25, 60, 100).expect("发送应成功"); // 端口 1 ch9
    conn.program_change(17, 3).expect("发送应成功"); // 端口 1 ch1
    let sent = sent.lock().expect("锁未 poison");
    assert_eq!(sent[0], [0x99, 60, 100], "note_on 应折叠到 ch9");
    assert_eq!(sent[1], [0xC1, 3, 0], "program_change 应折叠到 ch1");
}

/// REND-002 方案 B：默认 `set_percussion_mode` 为 no-op（外部设备无线协议消息）。
#[test]
fn default_set_percussion_mode_is_noop() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut conn = RawRecorder {
        sent: Arc::clone(&sent),
    };
    conn.set_percussion_mode(2, true)
        .expect("默认实现应成功且不发送任何字节");
    assert!(sent.lock().expect("锁未 poison").is_empty());
}

/// REND-002：默认 `release_all_dampers` 向 16 个线通道发 CC64=0。
#[test]
fn default_release_all_dampers_sends_cc64_all_channels() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut conn = RawRecorder {
        sent: Arc::clone(&sent),
    };
    conn.release_all_dampers().expect("默认实现应成功");
    let sent = sent.lock().expect("锁未 poison");
    assert_eq!(
        sent.len(),
        usize::from(MIDI_CHANNEL_COUNT),
        "应覆盖 16 通道"
    );
    for (ch, data) in sent.iter().enumerate() {
        assert_eq!(data[0], 0xB0 | ch as u8, "CC 状态字节与通道");
        assert_eq!(data[1], 64, "控制器应为 CC64");
        assert_eq!(data[2], 0, "值应为 0（释放延音）");
    }
}
