//! 播放键色增量扫描
//!
//! 包含：`PlaybackScanState` 缓存结构 + `update_playback_key_colors` 方法。
//!
//! # 2026-10-24 性能重构（PREF：区域高密度黑乐谱键色卡顿）
//!
//! ## 一、活跃集合 → 每键聚合
//!
//! 旧实现维护 `active_notes: Vec<(end_tick, key_offset, color)>`（24 B/活跃音符），
//! 每帧 `retain` 清理已结束音符 + 逐音符涂色，成本 O(活跃音符数)。在「区域音符
//! 密度超级高」的黑乐谱上（千万级音符同时发声），这一项足以吃掉整帧预算；
//! seek / 循环回绕时该 Vec 还会整体重分配（50M 同时活跃实测活跃集合 0.38 GB、
//! 稳态 110 ms/帧、seek 654 ms）。
//!
//! 本实现改为**每键聚合**。关键等价式：
//!
//! ```text
//! 键 k 亮  ⟺  ∃ 音符 start ≤ tick 且 end > tick
//!          ⟺  max{ end | start ≤ tick } > tick
//! ```
//!
//! 于是每帧只需把「本帧新开始的音符」并入 256 项的 `key_max_end`（单调取最大，
//! 永不回落，故**不需要任何到期清理结构**），再输出 256 项颜色表。
//!
//! 每帧成本 = O(本帧新开始音符数) + O(256)，与活跃音符数无关（仍与「本帧跨过的
//! 音符起点数」线性相关，这是数据本身的密度成本）；状态恒为定长 2 KB。
//!
//! ## 二、正向沿途快照：压掉反向 seek / 循环回绕的 O(全曲) 重扫
//!
//! 聚合对 tick 单调（`key_max_end` 只增不减），因此**回退后无法原地「减」回来**，
//! 必须重算。重算本可 O(已播放音符)，但 100M 音符工程实测 seek 达 87–238 ms
//! （已接近内存带宽地板：100M × 16 B 流式读取），而**循环播放每次回绕都要付一次**。
//!
//! 解法：正向播放时沿途按 tick 均匀采样快照（`SNAPSHOT_CAP` 个，每个约 2 KB），
//! 回退时从「≤ 目标 tick 的最近快照」起算，只需扫描快照间隔内的音符：
//!
//! - 快照采样成本：每跨过一个间隔拷贝 2 KB —— 相对逐音符扫描可忽略；
//! - 回退成本：O(全曲 / 64)，即 100M 音符下的约 1.5M 音符 ≈ 数 ms；
//! - 快照随「音符内容世代」失效，不会残留陈旧数据。
//!
//! 正向大步前跳同样受益：若跳变区间内已有快照，可直接跳到快照再补扫剩余部分。
//!
//! ## 三、删除跨全曲 `NoteSpatialIndex` 键色索引
//!
//! 该索引的构建是 O(N log N)（collect NoteRef + `to_vec` + 稳定排序 + 递归建树），
//! 正是 `impls/spatial.rs` 与 `note_ops/hit_test.rs` 中已记录的
//! 「1600W 音符建树 2-4s + 数百 MB 临时内存」同一瓶颈——空间索引已从渲染/命中
//! 路径退役，`playback.rs` 是最后一个消费者，此处一并退役。
//!
//! 收益对比（release 实测）：建一次索引 ≈ 200 ms / 百万音符，而它能省的只是一次
//! seek 的 O(已开始音符) 线性扫描（≈ 2.4 ns/音符）。需 seek 数十次才回本，且首帧
//! 必然吃满一次卡顿——对本功能是净亏。上面第二节的快照方案以更低成本达到了同样
//! 的目的（且不需要 O(N log N) 建树）。

use crate::{Editor, onion_track_color};

/// 键位槽数
///
/// `playback_key_colors` 为 256 键 × 4 字节；MIDI 音高只用到 0..=127，
/// 但保留 256 槽与既有输出缓冲布局一致（不做额外裁剪）。
const KEY_SLOTS: usize = 256;

/// 反向 seek 快照数量上限
///
/// 每份快照约 2 KB（`[u32; 256]` + `[[u8; 4]; 256]`），64 份约 131 KB。
/// 快照间隔由「文档总长 / `SNAPSHOT_CAP`」推导，故 64 份即可覆盖整条时间线。
const SNAPSHOT_CAP: usize = 64;

/// 播放位置快照：某 tick 上聚合结果的完整取值
///
/// `key_max_end` / `key_color` 覆盖「所有 `start_tick <= tick` 的音符」，
/// 因此可作为任意 `>= tick` 位置重算的合法基点（聚合对前缀单调）。
#[derive(Clone, Copy)]
pub(crate) struct PlaybackSnapshot {
    /// 快照对应的 tick（聚合覆盖到该 tick 为止）
    pub tick: u32,
    /// 该 tick 上的每键最晚结束 tick
    pub key_max_end: [u32; KEY_SLOTS],
    /// 该 tick 上的每键显示色
    pub key_color: [[u8; 4]; KEY_SLOTS],
}

/// 播放键色增量扫描状态
///
/// 所有字段都是**定长或 O(音轨数 / 快照数)**，不存在随播放进度增长的集合。
///
/// 触发重定位（换基点或从 tick 0 重扫）的条件：
/// - 首次调用（`notes_gen == None`）
/// - 音符数据世代变化（`EditorData::track_notes_gen`：编辑音符、装载新文档）
/// - 音轨数变化
/// - tick 回退（循环播放、用户拖动进度条向后）
#[derive(Clone)]
pub(crate) struct PlaybackScanState {
    /// 上次扫描到的 tick（用于判断 tick 方向）
    pub last_tick: f32,
    /// 每条音轨上次扫描到的索引（`partition_point` 结果缓存，仅正向推进）
    pub scan_idx: Vec<usize>,
    /// 每键「已开始音符中最晚结束 tick」；`> 当前 tick` 即该键亮
    ///
    /// 对「已开始音符」单调取最大、永不回落，因此正向播放只需并入新开始音符，
    /// 无需维护到期/清理结构。
    pub key_max_end: [u32; KEY_SLOTS],
    /// 每键当前显示色（由持有 `key_max_end` 的那个音符决定）
    ///
    /// 并列（同键同 `end_tick`）时保留**先处理者**：全量重扫按（音轨升序, 起始
    /// tick 升序）处理，增量扫描按帧序处理。并列且跨音轨的极端情况下两者可能
    /// 选中不同音符，但同属洋葱皮色系、视觉等价；**非并列情况两路径完全一致**
    /// （由 `test_keyboard_colors_incremental_matches_full_rescan` 守护）。
    pub key_color: [[u8; 4]; KEY_SLOTS],
    /// 上次扫描时的**音符内容世代**（`EditorData::track_notes_gen`）。
    ///
    /// 2026-10-17（审计 §五）修正：此前比对的是 `doc as *const MidiDocument` 地址，
    /// 两个方向都会漏判：
    ///
    /// 1. **就地编辑**（增/删/改音符）不改变文档地址 → 状态被判有效，此后重扫会拿陈旧数据着色；
    /// 2. **换文档**（旧文档 drop、新文档分配到同地址）→ 误判为同一份，残留上一份文档的键色。
    ///
    /// `track_notes_gen` 是代码库既有的音符内容世代（`mark_track_notes_changed`，
    /// 所有音符写入入口都必须 bump），且装载新文档时也会 bump
    /// （`Root::set_midi_document` / `host::editor_ops::set_midi_document`）。
    pub notes_gen: Option<u64>,
    /// 已扫描音轨数（检测音轨结构变化）
    ///
    /// 不直接用 `scan_idx.len()`：停止播放时 `scan_idx` 会被清空，
    /// 那是正常状态而非结构变化（否则每次停止都会误清快照）。
    pub track_count: usize,
    /// 正向播放沿途记录的快照（`tick` 升序）
    ///
    /// 供反向 seek / 循环回绕复用，把 O(全曲音符) 重扫压到 O(快照间隔内音符)。
    pub snapshots: Vec<PlaybackSnapshot>,
    /// 快照采样间隔（tick），由文档总长推导
    pub snapshot_stride: u32,
}

impl Default for PlaybackScanState {
    fn default() -> Self {
        Self {
            last_tick: 0.0,
            scan_idx: Vec::new(),
            key_max_end: [0; KEY_SLOTS],
            key_color: [[0; 4]; KEY_SLOTS],
            notes_gen: None,
            track_count: 0,
            snapshots: Vec::new(),
            snapshot_stride: 1,
        }
    }
}

impl PlaybackScanState {
    /// 把聚合并归零、扫描位置退回 tick 0（快照保留，由世代变化单独负责失效）
    fn reset_scan(&mut self) {
        self.last_tick = 0.0;
        self.key_max_end = [0; KEY_SLOTS];
        self.key_color = [[0; 4]; KEY_SLOTS];
        self.scan_idx.clear();
        self.scan_idx.resize(self.track_count, 0);
    }

    /// 保证 `scan_idx` 长度与音轨数一致
    fn ensure_scan_idx(&mut self, track_count: usize) {
        if self.scan_idx.len() != track_count {
            self.scan_idx.resize(track_count, 0);
        }
    }

    /// 挑选可复用的基点快照
    ///
    /// - `need_rebuild`（tick 回退）：任意 `tick <= 目标` 的最近快照都合法；
    /// - 正向推进：快照必须**不落后于当前扫描位置**（否则等于倒退），
    ///   且领先幅度不小于一个采样间隔才值得跳（否则白付一次恢复成本）。
    fn pick_base(&self, tick: u32, need_rebuild: bool, stride: u32) -> Option<usize> {
        let i = self.snapshots.iter().rposition(|s| s.tick <= tick)?;
        if need_rebuild {
            return Some(i);
        }
        let snap_tick = self.snapshots[i].tick;
        let current = self.last_tick.max(0.0) as u32;
        (snap_tick >= current && tick.saturating_sub(snap_tick) >= stride).then_some(i)
    }
}

impl Editor {
    /// 根据当前播放位置，计算每个 key 上被洋葱皮音符覆盖的颜色
    ///
    /// 直接从 `MidiDocument.track_notes()` 读取，数据已在 MIDI 导入时按 track 分组
    /// 并按 `start_tick` 升序排列。使用 `partition_point` 二分查找当前 tick 的活动音符。
    ///
    /// 播放停止时（`playback_position == 0.0`）清空颜色立即返回。
    /// 当 `playback_key_colors_enabled == false` 时直接返回。
    ///
    /// # 性能
    ///
    /// 每帧 O(本帧新开始音符数 + 音轨数 × log + 256)；反向 seek / 循环回绕
    /// O(快照间隔内音符)。**均与活跃音符数量无关**（见模块文档）。
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        // 停止播放（位置为 0）：清空颜色并归零聚合。
        // 快照**保留**——同一文档再次播放 / 回绕时可继续复用。
        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            self.playback_scan_state.reset_scan();
            return;
        }

        let tick = self.playback_position;
        let tick_u32 = tick as u32;

        // 一次性取出文档相关标量后立即释放文档不可变借用
        let (track_count, notes_gen, total_ticks) = {
            let Some(doc) = self.editor_state.data.document.as_ref() else {
                return;
            };
            (
                doc.track_count(),
                self.editor_state.data.track_notes_gen,
                doc.total_ticks,
            )
        };

        // 世代 / 音轨结构变化 → 聚合与快照全部作废（快照不得跨文档复用）
        if self.playback_scan_state.notes_gen != Some(notes_gen)
            || self.playback_scan_state.track_count != track_count
        {
            self.playback_scan_state.notes_gen = Some(notes_gen);
            self.playback_scan_state.track_count = track_count;
            self.playback_scan_state.snapshots.clear();
            self.playback_scan_state.snapshot_stride = (total_ticks / SNAPSHOT_CAP as u32).max(1);
            self.playback_scan_state.reset_scan();
        }
        self.playback_scan_state.ensure_scan_idx(track_count);

        // 基点选择：能复用快照就不必从 tick 0 重扫
        puffin::profile_scope!("key_color_rebase");
        let need_rebuild = tick < self.playback_scan_state.last_tick;
        let stride = self.playback_scan_state.snapshot_stride;
        match self
            .playback_scan_state
            .pick_base(tick_u32, need_rebuild, stride)
        {
            Some(i) => {
                // 快照为 Copy：先取出内容，脱离 `snapshots` 的不可变借用
                let snap = self.playback_scan_state.snapshots[i];
                self.playback_scan_state.key_max_end = snap.key_max_end;
                self.playback_scan_state.key_color = snap.key_color;
                self.playback_scan_state.last_tick = snap.tick as f32;
                // 扫描位置回到「首个 start_tick > snap.tick」处
                let data = &self.editor_state.data;
                for (ti, slot) in self.playback_scan_state.scan_idx.iter_mut().enumerate() {
                    *slot = data
                        .track_notes(ti)
                        .partition_point(snap.tick.wrapping_add(1));
                }
            }
            None => {
                if need_rebuild {
                    self.playback_scan_state.reset_scan();
                }
            }
        }

        // 把「本帧新开始的音符」并入每键聚合
        {
            puffin::profile_scope!("key_color_scan");
            let data = &self.editor_state.data;
            let state = &mut self.playback_scan_state;
            for track_idx in 0..track_count {
                let notes = data.track_notes(track_idx);
                if notes.is_empty() {
                    continue;
                }
                let start = state.scan_idx[track_idx];
                // ChunkedList::partition_point(tick+1) = 第一个 tick > tick_u32 的索引
                let end = notes.partition_point(tick_u32.wrapping_add(1));
                if end <= start {
                    continue;
                }
                state.scan_idx[track_idx] = end;
                let color = onion_track_color(track_idx);
                // iter_window 经块偏移直接定位，只访问 [start, end) 窗口
                for (_, n) in notes.iter_window(start, end) {
                    let end_tick = n.end_tick();
                    let key = n.key as usize;
                    // 单调取最大：只有「结束更晚」的音符才接管该键的颜色
                    if end_tick > state.key_max_end[key] {
                        state.key_max_end[key] = end_tick;
                        state.key_color[key] = color;
                    }
                }
            }
            state.last_tick = tick;
        }

        // 沿途记录回退快照（每跨过一个采样间隔拷贝约 2 KB，相对逐音符扫描可忽略）
        {
            puffin::profile_scope!("key_color_snapshot");
            let state = &mut self.playback_scan_state;
            let due = state
                .snapshots
                .last()
                .is_none_or(|s| tick_u32 >= s.tick.saturating_add(state.snapshot_stride));
            if due && state.snapshots.len() < SNAPSHOT_CAP {
                state.snapshots.push(PlaybackSnapshot {
                    tick: tick_u32,
                    key_max_end: state.key_max_end,
                    key_color: state.key_color,
                });
            }
        }

        // 依每键 max_end 输出 1024 字节颜色表（固定 256 次迭代）
        puffin::profile_scope!("key_color_emit");
        let mut colors = [0u8; 1024];
        for key in 0..KEY_SLOTS {
            if self.playback_scan_state.key_max_end[key] > tick_u32 {
                let offset = key * 4;
                colors[offset..offset + 4]
                    .copy_from_slice(&self.playback_scan_state.key_color[key]);
            }
        }
        // 无变化则不写回，避免下游无谓的重绘/纹理上传
        if colors != self.playback_key_colors {
            self.playback_key_colors = colors;
        }
    }

    /// 清空播放键色并重置增量扫描状态
    ///
    /// 停止播放（手动停止或自然结束）后调用，使键盘恢复为「无颜色」状态。
    ///
    /// 仅重置颜色数组与聚合，**不改变** `playback_position`：
    /// 自然结束时播放指示线应停留在结束位置（而非跳回 0），
    /// 手动停止由调用方负责复位 `playback_position`。
    ///
    /// 快照保留：同一文档的再次播放 / 循环回绕可继续复用，避免重付 O(全曲) 重扫。
    ///
    /// 当 `playback_key_colors_enabled == false` 时颜色数组本就是全零，调用安全无损。
    pub fn clear_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if self.playback_key_colors != [0u8; 1024] {
            self.playback_key_colors = [0u8; 1024];
        }
        self.playback_scan_state.reset_scan();
    }
}
