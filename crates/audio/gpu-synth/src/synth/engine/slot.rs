//! REF-LGS-010: 静态参数槽位管理。
//!
//! 声部在 `voices` 里的下标会随 `retain` 压缩而变化，但 GPU 的静态参数表
//! 按**槽位**索引：槽位在声部出生时分配、在声部移除时回收，因此静态参数
//! 只在分配槽位的那一块上传一次，之后每块只传动态字段。
//!
//! 本模块全部是纯函数（便于无 GPU 单测），`GpuSynth` 上的方法只是转发。

use super::*;

use crate::config::InterpolationMode;
use crate::gpu::{VoiceParamsDyn, VoiceParamsStatic};

impl GpuSynth {
    /// 移除声部并回收其静态槽位（替代裸 `voices.retain`）。
    pub(crate) fn retain_voices(&mut self, keep: impl FnMut(&Voice) -> bool) {
        let mut free = std::mem::take(&mut self.slot_free);
        recycle_slots(&mut self.voices, &mut free, keep);
        self.slot_free = free;
    }

    /// 清空声部列表并回收全部静态槽位（`voices.clear()` 语义）。
    pub(crate) fn clear_voices(&mut self) {
        let mut free = std::mem::take(&mut self.slot_free);
        free.extend(
            self.voices
                .iter()
                .map(|v| v.slot)
                .filter(|&s| s != VoiceParamsDyn::NO_SLOT),
        );
        self.voices.clear();
        self.slot_free = free;
    }

    /// 给尚未分配槽位的声部分配槽位，并写入静态表 CPU 镜像 + 上传队列。
    pub(crate) fn assign_static_slots(&mut self, interp: InterpolationMode) {
        assign_slots(
            &mut self.voices,
            &mut self.slot_free,
            &mut self.slot_count,
            &mut self.static_table,
            &mut self.dirty_slots,
            interp,
        );
    }

    /// 把本块脏槽位合并成连续区间（减少 belt 写调用）。
    pub(crate) fn coalesce_dirty_slots(&mut self) {
        coalesce_runs(&mut self.dirty_slots, &mut self.static_runs);
    }
}

/// `voices.retain(keep)` + 回收被移除声部的槽位（保持顺序与判定不变）。
fn recycle_slots(
    voices: &mut Vec<Voice>,
    slot_free: &mut Vec<u32>,
    mut keep: impl FnMut(&Voice) -> bool,
) {
    voices.retain(|v| {
        let alive = keep(v);
        if !alive && v.slot != VoiceParamsDyn::NO_SLOT {
            slot_free.push(v.slot);
        }
        alive
    });
}

/// 给 `slot == NO_SLOT` 的声部分配槽位（LIFO 复用），并写入静态表镜像。
///
/// 已分配槽位的声部不会被重写：静态参数在声部生命周期内不变
/// （`speed` 随弯音变化，属于动态字段）。
fn assign_slots(
    voices: &mut [Voice],
    slot_free: &mut Vec<u32>,
    slot_count: &mut u32,
    static_table: &mut Vec<VoiceParamsStatic>,
    dirty_slots: &mut Vec<u32>,
    interp: InterpolationMode,
) {
    for v in voices.iter_mut() {
        if v.slot != VoiceParamsDyn::NO_SLOT {
            continue;
        }
        let slot = match slot_free.pop() {
            Some(slot) => slot,
            None => {
                let slot = *slot_count;
                *slot_count += 1;
                slot
            }
        };
        v.slot = slot;
        let need = slot as usize + 1;
        if static_table.len() < need {
            static_table.resize(need, VoiceParamsStatic::zeroed());
        }
        static_table[slot as usize] = v.gpu_static(interp);
        dirty_slots.push(slot);
    }
}

/// 排序脏槽位并合并连续区间：`[3,4,5,9] -> [(3,3),(9,1)]`。
fn coalesce_runs(dirty_slots: &mut Vec<u32>, runs: &mut Vec<(u32, u32)>) {
    runs.clear();
    if dirty_slots.is_empty() {
        return;
    }
    dirty_slots.sort_unstable();
    let mut start = dirty_slots[0];
    let mut end = start + 1;
    for &slot in &dirty_slots[1..] {
        if slot == end {
            end += 1;
        } else {
            runs.push((start, end - start));
            start = slot;
            end = slot + 1;
        }
    }
    runs.push((start, end - start));
    dirty_slots.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::voices::test_voice;

    /// 槽位移除即回收、复用不再抬高水位；静态表长度只随高水位增长。
    #[test]
    fn slots_are_recycled_and_reused() {
        let mut voices = vec![test_voice(1, 60, 0, false), test_voice(2, 61, 0, false)];
        let mut free = Vec::new();
        let mut count = 0u32;
        let mut table = Vec::new();
        let mut dirty = Vec::new();
        assign_slots(
            &mut voices,
            &mut free,
            &mut count,
            &mut table,
            &mut dirty,
            InterpolationMode::Linear,
        );
        assert_eq!(count, 2);
        assert_eq!(table.len(), 2);
        assert_eq!(dirty.len(), 2);
        let first_slots: Vec<u32> = voices.iter().map(|v| v.slot).collect();

        // 移除第一个声部：槽位回到空闲表。
        recycle_slots(&mut voices, &mut free, |v| v.note_id != 1);
        assert_eq!(voices.len(), 1);
        assert_eq!(free.len(), 1);

        // 新声部复用回收的槽位，高水位与静态表都不增长。
        voices.push(test_voice(3, 62, 0, false));
        dirty.clear();
        assign_slots(
            &mut voices,
            &mut free,
            &mut count,
            &mut table,
            &mut dirty,
            InterpolationMode::Linear,
        );
        assert_eq!(count, 2, "recycled slot must be reused");
        assert_eq!(table.len(), 2);
        assert_eq!(dirty.len(), 1);
        assert!(first_slots.contains(&voices[1].slot));
    }

    /// 连续槽位合并成一个上传区间，跳号切成多段，脏表清空。
    #[test]
    fn dirty_slots_coalesce_into_runs() {
        let mut dirty = vec![5, 3, 4, 9];
        let mut runs = Vec::new();
        coalesce_runs(&mut dirty, &mut runs);
        assert_eq!(runs, vec![(3, 3), (9, 1)]);
        assert!(dirty.is_empty());
    }

    /// 静态表镜像保持与 `gpu_static` 一致（逐字段，含 loop/filter/channels）。
    #[test]
    fn static_table_mirrors_voice_fields() {
        let mut voices = vec![test_voice(1, 60, 0, false)];
        voices[0].speed = 2.5;
        voices[0].amp = 0.25;
        let mut free = Vec::new();
        let mut count = 0u32;
        let mut table = Vec::new();
        let mut dirty = Vec::new();
        assign_slots(
            &mut voices,
            &mut free,
            &mut count,
            &mut table,
            &mut dirty,
            InterpolationMode::Point64Sinc,
        );
        let expected = voices[0].gpu_static(InterpolationMode::Point64Sinc);
        assert_eq!(table[0].amp, expected.amp);
        assert_eq!(table[0].pan_l, expected.pan_l);
        assert_eq!(table[0].interp, 1);
        assert_eq!(table[0].loop_mode, expected.loop_mode);
        assert_eq!(table[0].sample_end, expected.sample_end);
    }
}
