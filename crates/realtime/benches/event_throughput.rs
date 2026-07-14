//! 实时事件处理吞吐量基准测试
//!
//! 模拟同时处理 10W / 50W / 100W 个不同完整音符事件（每个完整音符 = NoteOn + NoteOff）。
//! 每轮测试重复 3 次，输出平均耗时。

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use lumino_realtime::engine::RealtimeEventEngine;
use lumino_realtime::{ChannelAudioEvent, ChannelEvent, SynthEvent, XSynthRealtimeConfig};
use xsynth_core::AudioStreamParams;

/// 测试场景：完整音符数量。
const SCENARIOS: &[usize] = &[100_000, 500_000, 1_000_000];

/// 每轮测试重复次数。
const REPEATS: usize = 3;

/// 生成指定数量的“不同”完整音符事件。
///
/// 每个音符包含一个 NoteOn 和一个 NoteOff，分布在 16 个通道与 128 个键上，
/// 以保证事件对象的字段内容不完全相同，贴近真实 MIDI 流的多样性。
fn generate_full_note_events(count: usize) -> Vec<SynthEvent> {
    let mut events = Vec::with_capacity(count.saturating_mul(2));
    for i in 0..count {
        let channel = (i % 16) as u32;
        let key = (i % 128) as u8;
        let vel = (60 + (i % 68)) as u8;

        events.push(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key, vel }),
        ));
        events.push(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key }),
        ));
    }
    events
}

/// 单次测试：发送所有事件并驱动引擎处理完毕，返回总耗时。
fn run_once(count: usize) -> Duration {
    let config = XSynthRealtimeConfig::default();
    let stream_params = AudioStreamParams::new(44_100, 2.into());
    let voice_count = Arc::new(AtomicU64::new(0));

    let mut engine = RealtimeEventEngine::new(config, stream_params, voice_count);
    let events = generate_full_note_events(count);
    let total_events = count.saturating_mul(2) as u64;

    let start = Instant::now();
    engine.send_events(events).expect("批量发送事件不应失败");

    let mut processed = 0u64;
    while processed < total_events {
        engine.render_frame();
        processed += engine.perf_stats().last_event_count;
    }

    start.elapsed()
}

/// 计算一组耗时的平均值。
fn average(durations: &[Duration]) -> Duration {
    let total_ns: u64 = durations.iter().map(|d| d.as_nanos() as u64).sum();
    Duration::from_nanos(total_ns / durations.len().max(1) as u64)
}

fn main() {
    println!("Lumino Realtime 事件吞吐量基准测试");
    println!("=====================================");
    println!("每个完整音符 = NoteOn + NoteOff，分布在 16 个通道 / 128 个键");
    println!("每轮测试重复 {REPEATS} 次，取平均值");
    println!();

    for &count in SCENARIOS {
        // 预热一次，排除首次分配与缓存冷启动的影响。
        let _ = run_once(count);

        let mut durations = Vec::with_capacity(REPEATS);
        for _ in 0..REPEATS {
            durations.push(run_once(count));
        }

        let avg = average(&durations);
        let total_events = count.saturating_mul(2);

        println!(
            "完整音符数: {:>10} | 总事件数: {:>10} | 平均耗时: {:>10.3} ms",
            count,
            total_events,
            avg.as_secs_f64() * 1000.0
        );
    }
}
