//! `XSynth::init_synth` 相关纯逻辑测试（从 `init.rs` 拆分，保持文件 < 400 行）。

use super::*;

/// REND-002 零行为变化基线：单端口 → Midi/16 通道。
#[test]
fn rt_format_zero_is_midi_baseline() {
    assert_eq!(rt_format(0), SynthFormat::Midi);
}

/// REND-002：多端口 → Custom 通道数；超产品上限折叠到 16 端口/256 通道。
#[test]
fn rt_format_multi_port_uses_custom_channels() {
    assert_eq!(rt_format(6), SynthFormat::Custom { channels: 112 });
    assert_eq!(
        rt_format(127),
        SynthFormat::Custom { channels: 256 },
        "超上限端口折叠"
    );
}

/// #102 N-5：轻量复位事件序列——单端口：16 通道退出打击乐 + SystemReset，
/// 不额外恢复 ch9（`SynthFormat::Midi` 自动开启）。
#[test]
fn reset_events_single_port_clear_then_system_reset() {
    let events = reset_channel_state_events(0);
    assert_eq!(events.len(), 16 + 1, "16 通道退出 + SystemReset");
    for (ch, event) in events[..16].iter().enumerate() {
        assert_eq!(
            percussion_off_channel(event),
            Some(ch as u32),
            "第 {ch} 条应为通道 {ch} 退出打击乐"
        );
    }
    assert!(is_system_reset(&events[16]), "SystemReset 收尾");
    assert!(
        events.iter().all(|e| percussion_on_channel(e).is_none()),
        "单端口不得重复下发 ch9 恢复"
    );
}

/// #102 N-5：多端口：逐通道退出 → SystemReset → 按端口 ch9 恢复；
/// 恢复必须在 SystemReset 之后（无论 SystemReset 是否清模态，终态正确）。
#[test]
fn reset_events_multi_port_restore_percussion_after_reset() {
    let max_port = 6u8;
    let channels = channels_for_max_port_clamped(max_port) as usize; // 112
    let events = reset_channel_state_events(max_port);
    let restored = percussion_channels(max_port);
    assert_eq!(events.len(), channels + 1 + restored.len());

    for (ch, event) in events[..channels].iter().enumerate() {
        assert_eq!(
            percussion_off_channel(event),
            Some(ch as u32),
            "出口段应按全局通道序逐通道退出"
        );
    }
    assert!(
        is_system_reset(&events[channels]),
        "SystemReset 位于出口段之后"
    );
    let on: Vec<u32> = events[channels + 1..]
        .iter()
        .filter_map(percussion_on_channel)
        .collect();
    assert_eq!(on, restored, "SystemReset 之后逐端口恢复 ch9");
}

/// 若事件为“通道退出打击乐”，返回通道号。
fn percussion_off_channel(event: &SynthEvent) -> Option<u32> {
    match event {
        SynthEvent::Channel(
            ch,
            ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(false)),
        ) => Some(*ch),
        _ => None,
    }
}

/// 若事件为“通道恢复打击乐”，返回通道号。
fn percussion_on_channel(event: &SynthEvent) -> Option<u32> {
    match event {
        SynthEvent::Channel(
            ch,
            ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(true)),
        ) => Some(*ch),
        _ => None,
    }
}

/// 是否为全通道 SystemReset。
fn is_system_reset(event: &SynthEvent) -> bool {
    matches!(
        event,
        SynthEvent::AllChannels(ChannelEvent::Audio(ChannelAudioEvent::SystemReset))
    )
}
