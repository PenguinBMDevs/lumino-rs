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
//! 密度超级高」的黑乐谱上（十万~百万级音符同时发声），这一项足以吃掉整帧预算；
//! seek / 循环回绕时该 Vec 还会瞬时涨到全曲规模（数百 MB 分配与首次触页）。
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
//! 每帧成本 = O(本帧新开始音符数) + O(256)，**与活跃音符数、与已播放总量均无关**。
//!
//! ## 二、删除跨全曲 `NoteSpatialIndex` 键色索引
//!
//! 该索引的构建是 O(N log N)（collect NoteRef + `to_vec` + 稳定排序 + 递归建树），
//! 正是 `impls/spatial.rs` 与 `note_ops/hit_test.rs` 中已记录的
//! 「1600W 音符建树 2-4s + 数百 MB 临时内存」同一瓶颈——空间索引已从渲染/命中
//! 路径退役，`playback.rs` 是最后一个消费者，此处一并退役。
//!
//! 收益对比（release 实测）：建一次索引 ≈ 200 ms / 百万音符，而它能省的只是一次
//! seek 的 O(已开始音符) 线性扫描（≈ 2.4 ns/音符，百万音符约 2.4 ms）。
//! 需 seek 约 80 次才回本，且首帧必然吃满一次卡顿——对本功能是净亏。
//! 顺带消除 `PLAYBACK_KEY_INDEX_MAX_NOTES = 2M` 造成的悬崖：超过阈值时白花建树
//! 时间后仍退回 O(已开始音符)，且每次 seek/循环都要重付。

use crate::{Editor, onion_track_color};

/// 键位槽数
///
/// `playback_key_colors` 为 256 键 × 4 字节；MIDI 音高只用到 0..=127，
/// 但保留 256 槽与既有输出缓冲布局一致（不做额外裁剪）。
const KEY_SLOTS: usize = 256;

/// 播放键色增量扫描状态
///
/// 用于避免 `update_playback_key_colors` 每帧 O(N) 全量扫描。所有字段都是
/// **定长或 O(音轨数)**，不存在随播放进度增长的集合。
///
/// 触发全量重扫（`scan_idx` 归零、聚合清空）的条件：
/// - 首次调用（`notes_gen == None`）
/// - 音符数据世代变化（`EditorData::track_notes_gen`：编辑音符、装载新文档）
/// - tick 回退（循环播放、用户拖动进度条向后）
/// - 音轨数变化
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
}

impl Default for PlaybackScanState {
    fn default() -> Self {
        Self {
            last_tick: 0.0,
            scan_idx: Vec::new(),
            key_max_end: [0; KEY_SLOTS],
            key_color: [[0; 4]; KEY_SLOTS],
            notes_gen: None,
        }
    }
}

impl PlaybackScanState {
    /// 清空聚合结果并把扫描位置退回起点（下次从 tick 0 重扫）
    fn reset_to_start(&mut self, track_count: usize) {
        self.last_tick = 0.0;
        self.scan_idx.clear();
        self.scan_idx.resize(track_count, 0);
        self.key_max_end = [0; KEY_SLOTS];
        self.key_color = [[0; 4]; KEY_SLOTS];
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
    /// 每帧 O(本帧新开始音符数 + 音轨数 × log) ，输出固定 256 项；**与活跃音符
    /// 数量、与已播放音符总量均无关**（见模块文档的等价式推导）。
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            // 停止时重置扫描状态，下次播放从头开始。
            self.playback_scan_state.reset_to_start(0);
            return;
        }

        let tick = self.playback_position;
        let tick_u32 = tick as u32;

        // 一次性取出文档相关标量后立即释放文档不可变借用
        let (track_count, notes_gen) = {
            let Some(doc) = self.editor_state.data.document.as_ref() else {
                return;
            };
            (doc.track_count(), self.editor_state.data.track_notes_gen)
        };

        // 失效判定：世代变化 / tick 回退 / 音轨数变化 → 从 tick 0 重扫。
        //
        // 不再按「大幅前跳」触发重扫：`key_max_end` 是「已开始音符 end 的最大值」，
        // 对前缀单调，正向跳过多远都只需并入被跳过区间内的音符，重扫并无正确性收益。
        let need_rebuild = self.playback_scan_state.notes_gen != Some(notes_gen)
            || tick < self.playback_scan_state.last_tick
            || self.playback_scan_state.scan_idx.len() != track_count;
        if need_rebuild {
            self.playback_scan_state.notes_gen = Some(notes_gen);
            self.playback_scan_state.reset_to_start(track_count);
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
    /// 仅重置颜色数组与扫描缓存，**不改变** `playback_position`：
    /// 自然结束时播放指示线应停留在结束位置（而非跳回 0），
    /// 手动停止由调用方负责复位 `playback_position`。
    ///
    /// 当 `playback_key_colors_enabled == false` 时颜色数组本就是全零，调用安全无损。
    pub fn clear_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if self.playback_key_colors != [0u8; 1024] {
            self.playback_key_colors = [0u8; 1024];
        }
        self.playback_scan_state.reset_to_start(0);
    }
}
