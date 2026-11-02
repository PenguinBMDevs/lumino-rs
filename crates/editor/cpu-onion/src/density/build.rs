//! 密度计数基础层的规划与并行统计。

use std::thread;

use lumino_midi_model::MidiDocument;

use super::{BASE_BIN_TICKS, MAX_BASE_BINS};

/// 基础层规划：桶宽与 bin 数。
pub(super) fn base_plan(total_ticks: u32) -> (u32, usize) {
    let needed = (u64::from(total_ticks) / MAX_BASE_BINS as u64).saturating_add(1);
    let mut bin_ticks = BASE_BIN_TICKS;
    while u64::from(bin_ticks) < needed {
        bin_ticks = bin_ticks.saturating_mul(2);
    }
    let bins = ((u64::from(total_ticks) / u64::from(bin_ticks)) as usize)
        .saturating_add(1)
        .clamp(1, MAX_BASE_BINS);
    (bin_ticks, bins)
}

/// 并行统计基础层；返回 `(cells, 最大 start_tick, 最大 end_tick)`。
pub(super) fn count_base(
    document: &MidiDocument,
    bin_ticks: u32,
    bins: usize,
    keys: usize,
    workers: usize,
) -> (Vec<u16>, u32, u32) {
    let stride = bins.saturating_mul(keys);
    let mut base = vec![0u16; stride];
    let track_count = document.notes.len();
    if stride == 0 || track_count == 0 {
        return (base, 0, 0);
    }
    let workers = workers.max(1).min(track_count);
    let chunk = track_count.div_ceil(workers).max(1);
    let mut max_start = 0u32;
    let mut max_end = 0u32;
    thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..workers {
            let start = worker * chunk;
            let end = (start + chunk).min(track_count);
            if start >= end {
                continue;
            }
            handles.push(scope.spawn(move || {
                let mut local = vec![0u16; stride];
                let mut local_max_start = 0u32;
                let mut local_max_end = 0u32;
                for track in start..end {
                    for note in document.track_notes(track).iter() {
                        local_max_start = local_max_start.max(note.start_tick);
                        local_max_end = local_max_end.max(note.end_tick);
                        if note.key as usize >= keys {
                            continue;
                        }
                        let bin = (note.start_tick / bin_ticks) as usize;
                        if bin >= bins {
                            continue;
                        }
                        let offset = bin * keys + note.key as usize;
                        if let Some(slot) = local.get_mut(offset) {
                            *slot = slot.saturating_add(1);
                        }
                    }
                }
                (local, local_max_start, local_max_end)
            }));
        }
        for handle in handles {
            if let Ok((local, shard_start, shard_end)) = handle.join() {
                max_start = max_start.max(shard_start);
                max_end = max_end.max(shard_end);
                for (destination, source) in base.iter_mut().zip(local) {
                    *destination = destination.saturating_add(source);
                }
            }
        }
    });
    (base, max_start, max_end)
}
