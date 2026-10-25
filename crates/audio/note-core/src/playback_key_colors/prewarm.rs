//! 播放键色快照预热
//!
//! 见 [`super`] 模块文档第三节。这里把「整条时间线按 tick 均匀采样成快照」
//! 的工作摊到多帧：每帧最多消费 [`DEFAULT_PREWARM_NOTE_BUDGET`] 个音符，
//! 因此不会在主线程上出现 O(全曲音符) 的尖峰。
//!
//! 预热用的聚合是**独立于播放状态的一份**（`scan_idx` / `key_max_end` /
//! `key_track` 各自成册），只把结果（快照）并入共享查询；因此它可以在播放
//! 头位于任意位置时继续推进，不与实时扫描相互干扰。

use lumino_midi_model::MidiDocument;

use super::{KEY_SLOTS, KeyColorSnapshot};

/// 单帧预热可消费的音符数预算
///
/// 200 000 音符 ≈ 0.5 ms（release，2.4 ns/音符），占 60 fps 帧预算的 3%。
/// 1 亿音符工程约需 500 帧（≈8 s 后台）完成全时间线预热。
pub const DEFAULT_PREWARM_NOTE_BUDGET: usize = 200_000;

/// 预热快照数量上限
///
/// 与播放沿途快照同量级（64 份 × 约 1.5 KB ≈ 96 KB），两者是**互补**的：
/// 预热负责「还没播过的区间」，沿途快照负责「刚播过的区间」。
const PREWARM_SNAPSHOT_CAP: usize = 64;

/// 预热状态
pub(super) struct Prewarm {
    /// 每轨「已并入聚合」的音符数
    pub(super) scan_idx: Vec<usize>,
    /// 每键已并入音符的最晚结束 tick
    pub(super) key_max_end: [u32; KEY_SLOTS],
    /// 每键最晚结束音符所属音轨
    pub(super) key_track: [u16; KEY_SLOTS],
    /// 已产出的快照（tick 升序）
    pub(super) snapshots: Vec<KeyColorSnapshot>,
    /// 当前窗口上界（= 下一个快照的 tick）
    window_end: u32,
    /// 是否已覆盖整条时间线（或达上限）
    pub(super) done: bool,
}

impl Default for Prewarm {
    fn default() -> Self {
        Self {
            scan_idx: Vec::new(),
            key_max_end: [0; KEY_SLOTS],
            key_track: [0; KEY_SLOTS],
            snapshots: Vec::new(),
            window_end: 0,
            done: false,
        }
    }
}

impl Prewarm {
    /// 清空预热进度与产出的快照
    pub(super) fn reset(&mut self) {
        self.scan_idx.clear();
        self.key_max_end = [0; KEY_SLOTS];
        self.key_track = [0; KEY_SLOTS];
        self.snapshots.clear();
        self.window_end = 0;
        self.done = false;
    }

    /// 推进预热，最多消费 `note_budget` 个音符
    ///
    /// 返回 `true` 表示仍需继续（尚未覆盖整条时间线）。
    ///
    /// 复杂度：每步 O(音轨数 × log) 定位 + O(实际消费音符数)。
    ///
    /// **不需要取色回调**：快照只记录「该键的最晚结束 tick 与其所属音轨」，
    /// 颜色在 emit 时按当前取色策略解析（见 [`super`] 模块文档第四节）。
    pub(super) fn step(
        &mut self,
        doc: &MidiDocument,
        total_ticks: u32,
        stride: u32,
        note_budget: usize,
    ) -> bool {
        if self.done {
            return false;
        }
        let track_count = doc.track_count();
        if self.scan_idx.len() != track_count {
            // 音轨结构变化：从头预热
            self.reset();
            self.scan_idx.resize(track_count, 0);
        }
        let stride = stride.max(1);
        if self.window_end == 0 {
            self.window_end = stride;
        }

        let mut left = note_budget.max(1);
        loop {
            // 当前窗口是否已被所有轨聚合完（快照要求「所有 start <= window_end」）
            let mut window_done = true;
            for ti in 0..track_count {
                let notes = doc.track_notes(ti);
                if notes.is_empty() {
                    continue;
                }
                if self.scan_idx[ti] < notes.partition_point(self.window_end.wrapping_add(1)) {
                    window_done = false;
                    break;
                }
            }

            if window_done {
                if self.snapshots.len() < PREWARM_SNAPSHOT_CAP {
                    self.snapshots.push(KeyColorSnapshot {
                        tick: self.window_end,
                        key_max_end: self.key_max_end,
                        key_track: self.key_track,
                    });
                }
                if self.snapshots.len() >= PREWARM_SNAPSHOT_CAP || self.window_end >= total_ticks {
                    self.done = true;
                    return false;
                }
                self.window_end = self.window_end.saturating_add(stride);
                continue;
            }

            if left == 0 {
                return true;
            }

            let mut progressed = false;
            for ti in 0..track_count {
                let notes = doc.track_notes(ti);
                if notes.is_empty() {
                    continue;
                }
                let start = self.scan_idx[ti];
                let end = notes.partition_point(self.window_end.wrapping_add(1));
                if end <= start {
                    continue;
                }
                let take = (end - start).min(left);
                for (_, n) in notes.iter_window(start, start + take) {
                    let end_tick = n.end_tick();
                    let key = n.key as usize;
                    if end_tick > self.key_max_end[key] {
                        self.key_max_end[key] = end_tick;
                        self.key_track[key] = ti as u16;
                    }
                }
                self.scan_idx[ti] = start + take;
                left -= take;
                progressed = true;
                if left == 0 {
                    return true;
                }
            }
            if !progressed {
                // 不可达：`window_done == false` 说明至少一轨仍有待消费音符
                self.done = true;
                return false;
            }
        }
    }
}
