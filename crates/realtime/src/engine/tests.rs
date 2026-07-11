//! 实时事件处理引擎单元测试

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use xsynth_core::AudioStreamParams;

use super::RealtimeEventEngine;
use crate::config::XSynthRealtimeConfig;
use crate::events::{ChannelAudioEvent, ChannelEvent, SynthEvent};

fn test_engine() -> RealtimeEventEngine {
    let config = XSynthRealtimeConfig::default();
    let stream_params = AudioStreamParams::new(44_100, 2.into());
    RealtimeEventEngine::new(config, stream_params, Arc::new(AtomicU64::new(0)))
}

#[test]
fn send_and_render_single_event() {
    let mut engine = test_engine();
    engine.send_event(SynthEvent::Channel(
        0,
        ChannelEvent::Audio(ChannelAudioEvent::AllNotesOff),
    ));

    let buf = engine.render_frame().expect("应返回渲染缓冲区");
    assert_eq!(buf.len(), engine.render_window() * 2);
}

#[test]
fn send_events_batch_updates_perf_counter() {
    let mut engine = test_engine();
    let events: Vec<SynthEvent> = (0..10)
        .map(|i| {
            SynthEvent::Channel(
                0,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                    key: i as u8,
                    vel: 80,
                }),
            )
        })
        .collect();

    engine.send_events(events).expect("批量发送应成功");
    let _ = engine.render_frame();

    assert!(engine.perf_stats().last_event_count >= 10);
}
