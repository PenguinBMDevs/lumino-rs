use super::*;
use crate::constants::{STATUS_CONTROL_CHANGE, STATUS_NOTE_ON};

#[test]
fn channel_parts_expands_rend_002_global_channel() {
    assert_eq!(channel_parts(0), (0, 0));
    assert_eq!(channel_parts(15), (0, 15));
    assert_eq!(channel_parts(16), (1, 0));
    assert_eq!(channel_parts(31), (1, 15));
    assert_eq!(channel_parts(255), (15, 15));
    // 越界全局通道按 u8 上限钳制，不能回绕到 port 0。
    assert_eq!(channel_parts(256), (15, 15));
}

#[test]
fn short_message_keeps_status_and_payload() {
    let (port, message) = short_message_for(0, STATUS_NOTE_ON, 60, 100);
    assert_eq!(port, 0);
    assert_eq!(message, 0x90 | (60 << 8) | (100 << 16));

    // 全局通道 15 = port 0 / channel 15，状态字节低 4 位必须带通道。
    let (port, message) = short_message_for(15, STATUS_NOTE_ON, 60, 100);
    assert_eq!(port, 0);
    assert_eq!(message, 0x9F | (60 << 8) | (100 << 16));

    let (port, message) = short_message_for(16 + 3, STATUS_CONTROL_CHANGE, 64, 0);
    assert_eq!(port, 1);
    assert_eq!(message, 0xB3 | (64 << 8));
}

#[test]
fn pitch_bend_matches_default_mapping() {
    assert_eq!(pitch_bend_value(-1.0), (0, 0));
    // 0.0 → 8192 → lsb 0 / msb 64（与默认实现同一公式）
    assert_eq!(pitch_bend_value(0.0), (0, 64));
    assert_eq!(pitch_bend_value(1.0), (127, 127));
}
