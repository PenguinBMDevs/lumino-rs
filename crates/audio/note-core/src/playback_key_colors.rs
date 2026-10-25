//! 播放键色扫描——编辑器实时播放与视频导出共用的**唯一实现**
//!
//! # 一、等价式：为什么不需要「活跃音符集合」
//!
//! ```text
//! 键 k 亮  ⟺  ∃ 音符 start ≤ tick 且 end > tick
//!          ⟺  max{ end | start ≤ tick } > tick
//! ```
//!
//! 因此每帧只需把「本帧新开始的音符」并入 256 项的 `key_max_end`（对前缀单调
//! 取最大、永不回落，故**不需要任何到期清理结构**），再输出 256 项颜色表。
//!
//! 每帧成本 = O(本帧新开始音符数) + O(256)。**与「同时发声音符数」无关**：
//! 旧实现（维护活跃集合 + 逐音符涂色）在 5 000 万音符同时发声时实测
//! 110 ms/帧、seek 654 ms；本实现 1.9 ms/帧、状态恒为约 1.5 KB。
//!
//! # 二、回退：沿途快照
//!
//! 聚合对 tick 单调，回退后无法原地「减」回来，必须重算。重算本可
//! O(已播放音符)，但 1 亿音符工程实测 seek 达 87–238 ms（已接近内存带宽地板），
//! 而**循环播放每次回绕都要付一次**。
//!
//! 解法：正向播放时沿途按 tick 均匀采样快照（[`KEY_COLOR_SNAPSHOT_CAP`] 份，
//! 每份约 1.5 KB），回退时从「≤ 目标 tick 的最近快照」起算，只补扫快照间隔内的
//! 音符。实测 1 亿音符工程循环回绕 **112.80 ms → 0.02 ms**。
//!
//! # 三、首次 seek：预热（见 [`prewarm`]）
//!
//! 若用户装载后**从未播过**就直接拖到中间，沿途快照尚不存在，首次 seek 仍是
//! O(已开始音符)（1 亿音符 135–238 ms）。[`PlaybackKeyColorState::prewarm_step`]
//! 用独立的一份聚合从 tick 0 向上按快照间隔推进，每帧只消费固定音符预算
//! （[`DEFAULT_PREWARM_NOTE_BUDGET`]），把「一次性冻结」摊成「后台预热」。
//!
//! # 四、颜色不参与聚合：只存音轨号
//!
//! 聚合里记录的是**「该键最晚结束音符所属的音轨号」**（`key_track`），颜色在
//! emit 时由调用方的取色回调解析。这样「取色策略变化」（切当前编辑轨导致主音轨
//! 语义改变、用户切换调色板）**不需要重扫**——同一份聚合按新策略重新上色即可。
//!
//! 若把颜色直接存进聚合，切轨/换调色板后键色会一直停在上一个策略上，
//! 直到某次内容世代变化触发全量重扫为止（即「键色不跟调色板走」的缺陷）。
//!
//! # 为什么不再构建跨全曲空间索引
//!
//! 旧实现为键色单独建了一棵 `NoteSpatialIndex`：O(N log N) 构建
//! （collect + 排序 + 递归建树，1.6 千万音符 2–4 s + 数百 MB），却只为省下一次
//! seek 的线性扫描。`impls/spatial.rs` 与 `note_ops/hit_test.rs` 已因同一瓶颈
//! 把该索引从渲染/命中路径退役，键色是最后一个消费者，已一并退役——上面第二、
//! 三节的快照方案以更低成本达到同样目的，且不需要 O(N log N) 建树。

mod prewarm;

use lumino_midi_model::MidiDocument;

pub use prewarm::DEFAULT_PREWARM_NOTE_BUDGET;

/// 键色输出缓冲区字节数（256 键 × RGBA）
pub const KEY_COLOR_BYTES: usize = 1024;

/// 键位槽数
///
/// 输出缓冲为 256 键 × 4 字节；MIDI 音高只用到 0..=127，保留 256 槽是为与既有
/// 缓冲布局一致（不做额外裁剪，`key = 200` 之类的越界数据也不会 panic）。
pub const KEY_SLOTS: usize = 256;

/// 播放沿途快照数量上限
///
/// 每份约 1.5 KB（`[u32; 256]` + `[u16; 256]`），64 份约 96 KB。
const KEY_COLOR_SNAPSHOT_CAP: usize = 64;

/// 某 tick 上键色聚合的完整取值
///
/// `key_max_end` / `key_track` 覆盖「所有 `start_tick <= tick` 的音符」，
/// 因此可作为任意 `>= tick` 位置重算的合法基点（聚合对前缀单调）。
#[derive(Clone, Copy)]
pub struct KeyColorSnapshot {
    /// 快照对应的 tick（聚合覆盖到该 tick 为止）
    pub tick: u32,
    /// 该 tick 上的每键最晚结束 tick
    pub key_max_end: [u32; KEY_SLOTS],
    /// 该 tick 上的每键「最晚结束音符」所属音轨
    pub key_track: [u16; KEY_SLOTS],
}

/// 播放键色扫描状态
///
/// 所有字段都是**定长或 O(音轨数 / 快照数)**，不存在随播放进度增长的集合。
///
/// 触发重定位（换快照基点或从 tick 0 重扫）的条件：
/// - 内容世代（`content_gen`）变化：编辑音符、装载新文档
/// - 音轨数变化
/// - tick 回退（循环播放、进度条后退）
///
/// **取色策略变化不触发重扫**（见模块文档第四节）。
pub struct PlaybackKeyColorState {
    /// 上次扫描到的 tick（用于判断 tick 方向）
    pub last_tick: u32,
    /// 每条音轨上次扫描到的索引（`partition_point` 结果缓存，仅正向推进）
    pub scan_idx: Vec<usize>,
    /// 每键「已开始音符中最晚结束 tick」；`> 当前 tick` 即该键亮
    pub key_max_end: [u32; KEY_SLOTS],
    /// 每键「最晚结束音符」所属音轨（颜色在 emit 时按当前策略解析）
    ///
    /// 并列（同键同 `end_tick`）时保留**先处理者**：重扫按（音轨升序, 起始 tick
    /// 升序）处理，增量按帧序处理。并列且跨音轨的极端情况下两者可能选中不同
    /// 音轨，但**同一取色策略下两条路径对非并列情况完全一致**
    /// （`test_keyboard_colors_incremental_matches_full_rescan` 守护）。
    pub key_track: [u16; KEY_SLOTS],
    /// 已聚合音轨数（检测音轨结构变化）
    pub track_count: usize,
    /// 快照采样间隔（tick），由文档总长推导
    pub snapshot_stride: u32,
    /// 调用方设定的**内容世代**；变化即作废聚合与全部快照
    pub content_gen: Option<u64>,
    /// 正向播放沿途记录的快照（`tick` 升序）
    pub snapshots: Vec<KeyColorSnapshot>,
    /// 后台预热状态（首次 seek 加速，见模块文档第三节）
    prewarm: prewarm::Prewarm,
}

impl Default for PlaybackKeyColorState {
    fn default() -> Self {
        Self {
            last_tick: 0,
            scan_idx: Vec::new(),
            key_max_end: [0; KEY_SLOTS],
            key_track: [0; KEY_SLOTS],
            track_count: 0,
            snapshot_stride: 1,
            content_gen: None,
            snapshots: Vec::new(),
            prewarm: prewarm::Prewarm::default(),
        }
    }
}

impl PlaybackKeyColorState {
    /// 创建空状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 把聚合并归零、扫描位置退回 tick 0，**保留**快照
    ///
    /// 用于「停止播放」：颜色要清空，但同一文档再次播放 / 回绕时快照仍可复用。
    pub fn reset_aggregate(&mut self) {
        self.last_tick = 0;
        self.key_max_end = [0; KEY_SLOTS];
        self.key_track = [0; KEY_SLOTS];
        self.scan_idx.clear();
        self.scan_idx.resize(self.track_count, 0);
    }

    /// 内容世代变化时全量作废（聚合 + 沿途快照 + 预热进度）
    pub fn invalidate(&mut self) {
        self.snapshots.clear();
        self.prewarm.reset();
        self.reset_aggregate();
    }

    /// 预热是否已完成（供诊断/测试）
    pub fn prewarm_done(&self) -> bool {
        self.prewarm.done
    }

    /// 预热快照份数（供诊断/测试）
    pub fn prewarm_snapshot_count(&self) -> usize {
        self.prewarm.snapshots.len()
    }

    fn ensure_scan_idx(&mut self, track_count: usize) {
        if self.scan_idx.len() != track_count {
            self.scan_idx.resize(track_count, 0);
        }
    }

    /// 挑选可复用的基点快照（沿途快照与预热快照取「tick 最大者」）
    ///
    /// - `need_rebuild`（tick 回退）：任意 `tick <= 目标` 的最近快照都合法；
    /// - 正向推进：只要快照**严格领先**当前扫描位置就值得跳——跳过去总比继续
    ///   从 `last_tick` 扫更省（恢复成本仅一次约 1.5 KB 拷贝 + 每轨一次二分）。
    ///
    /// 2026-10-25 修正：此处曾要求「领先幅度 ≥ 一个采样间隔」，导致预热快照
    /// 因剩余距离不足一个间隔而被整体拒绝，预热完全失效
    /// （实测首次 seek 143.80 ms → 132.16 ms，几乎无收益）。改为严格领先即用。
    fn pick_base(&self, tick: u32, need_rebuild: bool) -> Option<KeyColorSnapshot> {
        let recorded = self.snapshots.iter().rev().find(|s| s.tick <= tick);
        let warmed = self.prewarm.snapshots.iter().rev().find(|s| s.tick <= tick);
        let best = match (recorded, warmed) {
            (Some(a), Some(b)) => Some(if a.tick >= b.tick { *a } else { *b }),
            (Some(a), None) => Some(*a),
            (None, Some(b)) => Some(*b),
            (None, None) => None,
        }?;
        if need_rebuild {
            return Some(best);
        }
        (best.tick > self.last_tick).then_some(best)
    }

    /// 按当前 tick 重算键色并写入 `out`
    ///
    /// - `tick`：播放位置（tick 单位，已取整）
    /// - `content_gen`：音符内容世代；变化即全量作废（编辑器传 `track_notes_gen`，
    ///   视频导出等只读一次的场景可传固定值）
    /// - `color_of_track`：音轨 → RGBA。**只在 emit 阶段调用（256 次/帧）**，
    ///   取色策略由调用方决定（编辑器用 `lumino_extras::palette::note_track_color`
    ///   与卷帘音符同源）
    ///
    /// 输出与 `out` 逐字节相同时不写回，避免下游无谓重绘/纹理上传。
    pub fn update(
        &mut self,
        doc: &MidiDocument,
        tick: u32,
        content_gen: u64,
        color_of_track: &dyn Fn(usize) -> [u8; 4],
        out: &mut [u8; KEY_COLOR_BYTES],
    ) {
        puffin::profile_function!();
        let track_count = doc.track_count();
        let stride = (doc.total_ticks / KEY_COLOR_SNAPSHOT_CAP as u32).max(1);

        if self.content_gen != Some(content_gen) || self.track_count != track_count {
            self.content_gen = Some(content_gen);
            self.track_count = track_count;
            self.snapshot_stride = stride;
            self.invalidate();
        }
        self.ensure_scan_idx(track_count);

        // 基点选择：能复用快照就不必从 tick 0 重扫
        puffin::profile_scope!("key_color_rebase");
        let need_rebuild = tick < self.last_tick;
        if let Some(base) = self.pick_base(tick, need_rebuild) {
            self.key_max_end = base.key_max_end;
            self.key_track = base.key_track;
            self.last_tick = base.tick;
            for (ti, slot) in self.scan_idx.iter_mut().enumerate() {
                *slot = doc
                    .track_notes(ti)
                    .partition_point(base.tick.wrapping_add(1));
            }
        } else if need_rebuild {
            self.reset_aggregate();
        }

        // 把「本帧新开始的音符」并入每键聚合
        {
            puffin::profile_scope!("key_color_scan");
            for ti in 0..track_count {
                let notes = doc.track_notes(ti);
                if notes.is_empty() {
                    continue;
                }
                let start = self.scan_idx[ti];
                // ChunkedList::partition_point(tick+1) = 第一个 tick > tick 的索引
                let end = notes.partition_point(tick.wrapping_add(1));
                if end <= start {
                    continue;
                }
                self.scan_idx[ti] = end;
                let track_enc = ti as u16;
                // iter_window 经块偏移直接定位，只访问 [start, end) 窗口
                for (_, n) in notes.iter_window(start, end) {
                    let end_tick = n.end_tick();
                    let key = n.key as usize;
                    if end_tick > self.key_max_end[key] {
                        self.key_max_end[key] = end_tick;
                        self.key_track[key] = track_enc;
                    }
                }
            }
            self.last_tick = tick;
        }

        // 沿途记录回退快照（每跨一个采样间隔拷贝约 1.5 KB，相对逐音符扫描可忽略）
        {
            puffin::profile_scope!("key_color_snapshot");
            let due = self
                .snapshots
                .last()
                .is_none_or(|s| tick >= s.tick.saturating_add(self.snapshot_stride));
            if due && self.snapshots.len() < KEY_COLOR_SNAPSHOT_CAP {
                self.snapshots.push(KeyColorSnapshot {
                    tick,
                    key_max_end: self.key_max_end,
                    key_track: self.key_track,
                });
            }
        }

        self.emit(tick, color_of_track, out);
    }

    /// 依每键 max_end 输出颜色表（固定 256 次迭代 + 256 次取色）
    fn emit(
        &self,
        tick: u32,
        color_of_track: &dyn Fn(usize) -> [u8; 4],
        out: &mut [u8; KEY_COLOR_BYTES],
    ) {
        puffin::profile_scope!("key_color_emit");
        let mut colors = [0u8; KEY_COLOR_BYTES];
        for key in 0..KEY_SLOTS {
            if self.key_max_end[key] > tick {
                let offset = key * 4;
                colors[offset..offset + 4]
                    .copy_from_slice(&color_of_track(self.key_track[key] as usize));
            }
        }
        if colors != *out {
            *out = colors;
        }
    }

    /// 推进后台预热，最多消费 `note_budget` 个音符
    ///
    /// 返回 `true` 表示仍需继续。见模块文档第三节。
    ///
    /// 调用方应在**每帧**（而非仅播放时）驱动它，否则「装载后直接 seek」仍会
    /// 落到 O(已开始音符) 的重扫上。
    pub fn prewarm_step(&mut self, doc: &MidiDocument, note_budget: usize) -> bool {
        if self.prewarm.done || self.track_count != doc.track_count() {
            // 结构尚未由 `update` 锁定（或已变化）：本帧不预热，等 update 先跑
            return false;
        }
        puffin::profile_scope!("key_color_prewarm");
        let stride = (doc.total_ticks / KEY_COLOR_SNAPSHOT_CAP as u32).max(1);
        self.prewarm.step(doc, doc.total_ticks, stride, note_budget)
    }
}
