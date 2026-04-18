//! MIDI 增量时间处理

use midly::TrackEvent;

/// 将绝对时间转换为增量时间
pub fn convert_to_delta_times(events: &mut [TrackEvent<'_>]) {
    if events.is_empty() {
        return;
    }

    events.sort_by_key(|e| u32::from(e.delta));

    let mut last_tick: u32 = 0;
    for event in events.iter_mut() {
        let current_tick = u32::from(event.delta);
        let delta = current_tick.saturating_sub(last_tick);
        event.delta = delta.into();
        last_tick = current_tick;
    }
}
