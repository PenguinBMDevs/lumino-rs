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

/// 真机冒烟：需要本机安装 Maestro；默认不跑，显式 `--ignored` 执行。
#[test]
#[ignore = "需要本机安装 Maestro（0.2+）"]
fn maestro_real_smoke() {
    let maestro = Maestro::new(APP_ID, 15).expect("Maestro 应能加载并打开 16 端口引擎");
    let info = maestro.info().expect("应能读取 Info");
    assert_eq!(info.num_ports, 16, "num_ports 应为 max_port + 1");
    assert!(!info.midi2, "Lumino 使用 MIDI 1.0");

    let stats = maestro.stats_reader();
    assert!(stats.stats().is_some(), "引擎就绪后 stats 应可读");

    let mut output = maestro.open_output(0).expect("应能打开输出连接");
    // 只发 NoteOff / CC / reset，避免测试时发声。
    output
        .note_off(15 * 16 + 15, 127, 0)
        .expect("port 15 应有效");
    output.control_change(0, 64, 0).expect("CC64 应有效");
    output.all_notes_off().expect("notes_off 应成功");
    output.reset_control().expect("reset 应成功");
    output
        .reset_playback_stats()
        .expect("reset_playback_stats 应 no-op 成功");
    output.close();
}

/// 布局重建后，已创建的输出连接必须跟随新引擎（generation 热替换）。
#[test]
#[ignore = "需要本机安装 Maestro（0.2+）"]
fn maestro_layout_rebuild_keeps_existing_output_connection() {
    let mut maestro = Maestro::new(APP_ID, 0).expect("单端口引擎应能打开");
    let mut output = maestro.open_output(0).expect("应能打开输出连接");
    assert_eq!(maestro.info().expect("info").num_ports, 1);

    maestro
        .set_midi_port_layout(1)
        .expect("0 -> 1 布局应能重开引擎");
    assert_eq!(maestro.info().expect("info").num_ports, 2);

    // 同一个连接应已切到新引擎，并能访问 port 1。
    output
        .note_off(31, 60, 0)
        .expect("布局重建后 port 1 应有效");
    output.close();
}

/// 端口越界必须返回错误而不是 panic/静默串台。
#[test]
#[ignore = "需要本机安装 Maestro（0.2+）"]
fn maestro_port_out_of_range_is_error() {
    let maestro = Maestro::new(APP_ID, 0).expect("Maestro 应能加载并打开单端口引擎");
    let mut output = maestro.open_output(0).expect("应能打开输出连接");
    let error = output
        .note_on(16, 60, 100)
        .expect_err("单端口引擎的 port 1 应被 Maestro 拒绝");
    assert!(
        error.to_string().contains("send_short"),
        "错误应带上下文: {error}"
    );
}
