//! 播放键色增量扫描
//!
//! 包含：`PlaybackScanState` 缓存结构 + `update_playback_key_colors` 方法。
//!
//! 拆分原因：`editor_impl.rs` 接近 400 行限制，按职责拆分。

use crate::spatial_index::{NoteRef, NoteSpatialIndex};
use crate::{Editor, onion_track_color};
use lumino_midi_loader::MidiDocument;

/// 播放键色增量扫描状态
///
/// 用于避免 `update_playback_key_colors` 每帧 O(N) 全量扫描，
/// 其中 N = `start_tick <= 当前 tick` 的音符数（随播放时间线性增长）。
///
/// 通过缓存上次扫描位置和当前活跃音符集合，把每帧扫描量降到
/// O(新进入活跃的音符) + O(活跃音符 retain)。
///
/// 触发全量重建的条件：
/// - 首次调用（`doc_addr == None`）
/// - MIDI 文档切换（`doc_addr` 变化）
/// - tick 回退（循环播放、用户拖动进度条）
/// - tick 大幅前跳（用户 seek）
#[derive(Default)]
pub(crate) struct PlaybackScanState {
    /// 上次扫描到的 tick（用于判断 tick 方向）
    pub last_tick: f32,
    /// 每条音轨上次扫描到的索引（partition_point 结果缓存）
    pub scan_idx: Vec<usize>,
    /// 当前活跃音符缓存：(end_tick, key_offset_bytes, color)
    /// 每帧 retain 清理已结束音符，涂色时直接遍历此 Vec
    pub active_notes: Vec<(u32, usize, [u8; 4])>,
    /// 上次扫描时的 `Arc<MidiDocument>` 地址（用于检测文档切换）
    pub doc_addr: Option<usize>,
    /// 跨所有轨的播放键色空间索引（按 doc 缓存），用于全量重建时 O(活跃) 取活跃音符。
    /// `None` 表示未构建（超大工程超阈值）或已重置，调用方回退 O(end) 线性扫描。
    pub key_color_index: Option<PlaybackKeyIndex>,
}

/// 播放键色空间索引承载结构
///
/// 由 `rebuild_playback_key_index` 从文档所有轨音符一次性构建（按 doc 缓存），
/// 全量重建 `update_playback_key_colors` 时经 `NoteSpatialIndex::update_query`
/// 直接取得在 `tick` 活跃的音符，复杂度与已播放音符总量无关。
pub(crate) struct PlaybackKeyIndex {
    /// 跨轨音符空间索引（音符按 start_tick 排序，支持 [tick,tick] 区间活跃查询）
    pub index: NoteSpatialIndex,
    /// 全局音符索引 → 所属音轨（用于取洋葱皮轨道色）
    pub track_of: Vec<u16>,
    /// 全局音符索引 → 键位（用于着色偏移 key*4）
    pub key_of: Vec<u8>,
    /// 全局音符索引 → 结束 tick（用于增量 retain 清理）
    pub end_of: Vec<u32>,
}

/// 播放键色空间索引最大音符数
///
/// 超过此规模不构建索引，回退 O(end) 线性扫描（避免超大工程 O(N log N) 建树 /
/// 数百 MB 临时内存峰值）。该量级下线性扫描虽慢但安全、不阻塞。
const PLAYBACK_KEY_INDEX_MAX_NOTES: usize = 2_000_000;

/// 判定 seek 阈值（单位：tick）
///
/// 超过此阈值视为用户 seek，需要全量重建。
/// 保守取 5 秒等价 tick（480 PPQ @ 120BPM ≈ 960 tick/秒 → 4800 tick）
const SEEK_THRESHOLD_TICKS: f32 = 5000.0;

impl Editor {
    /// 根据当前播放位置，计算每个 key 上被洋葱皮音符覆盖的颜色
    ///
    /// 直接从 `MidiDocument.track_notes()` 读取，数据已在 MIDI 导入时按 track 分组
    /// 并按 `start_tick` 升序排列。使用 `partition_point` 二分查找当前 tick 的活动音符。
    ///
    /// 播放停止时（`playback_position == 0.0`）清空颜色立即返回。
    /// 当 `playback_key_colors_enabled == false` 时直接返回。
    ///
    /// # 性能策略（增量扫描 + 跨轨空间索引）
    ///
    /// 朴素实现遍历 `[0, end)` 区间所有音符，复杂度 O(end)，其中 `end` 随播放时间
    /// 线性增长——百万级音符的 MIDI 播放数分钟后，每次全量重建可达数百 ms。
    ///
    /// 本方法维护 `PlaybackScanState` 缓存上次扫描位置和当前活跃音符集合：
    /// - 正常播放：增量扫描新进入的音符 + retain 清理已结束音符，每帧 O(活跃音符数)
    /// - seek / 循环回绕 / 文档切换：触发全量重建
    ///
    /// 全量重建优先使用 `key_color_index`（跨所有轨空间索引），经
    /// `update_query(tick, tick, 0, 255, _)` 直接取得在 `tick` 活跃的音符，
    /// 复杂度 O(活跃音符数)，与已播放音符总量无关。仅文档切换时构建一次索引。
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
            // 注意：保留 key_color_index（仅依赖文档音符，与播放进度无关），
            // 避免重启后首帧全量重建退回 O(end) 线性扫描。
            self.playback_scan_state.last_tick = 0.0;
            self.playback_scan_state.scan_idx.clear();
            self.playback_scan_state.active_notes.clear();
            return;
        }

        let tick = self.playback_position;
        let tick_u32 = tick as u32;

        // 一次性取出文档相关标量后立即释放文档不可变借用，避免在后续 `&mut self`
        // 调用（`rebuild_playback_key_index`）时因同时存在借用而触发借用冲突。
        let (track_count, current_doc_addr, need_full_rebuild) = {
            let Some(doc) = self.editor_state.data.document.as_ref() else {
                return;
            };
            let current_doc_addr = doc as *const MidiDocument as usize;
            let doc_changed = self.playback_scan_state.doc_addr != Some(current_doc_addr);
            let last_tick = self.playback_scan_state.last_tick;
            let need_full_rebuild =
                doc_changed || tick < last_tick || (tick - last_tick) > SEEK_THRESHOLD_TICKS;
            (doc.track_count(), current_doc_addr, need_full_rebuild)
        };

        if need_full_rebuild {
            // 仅在文档切换时重建索引（按 doc 缓存，避免每次 seek/循环重复建树）
            if self.playback_scan_state.doc_addr != Some(current_doc_addr) {
                self.rebuild_playback_key_index(track_count, current_doc_addr);
            }

            let Some(doc) = self.editor_state.data.document.as_ref() else {
                return;
            };

            // 全量重建需保证 scan_idx 容量与 track_count 对齐（首次/重置后长度为 0）
            if self.playback_scan_state.scan_idx.len() != track_count {
                self.playback_scan_state.scan_idx = vec![0; track_count];
            }

            if let Some(ki) = &self.playback_scan_state.key_color_index {
                // O(活跃) 路径：直接查询在 [tick, tick] 区间活跃的音符
                let mut result = Vec::new();
                ki.index.update_query(tick, tick, 0, 255, &mut result);
                self.playback_scan_state.active_notes.clear();
                for &gi in &result {
                    let end_tick = ki.end_of[gi];
                    // 与原 O(end) 路径一致：仅 end_tick > tick 的音符视为活跃
                    if end_tick <= tick_u32 {
                        continue;
                    }
                    let track = ki.track_of[gi] as usize;
                    let key = ki.key_of[gi] as usize;
                    let color = onion_track_color(track);
                    self.playback_scan_state
                        .active_notes
                        .push((end_tick, key * 4, color));
                }
                // 维持 scan_idx 不变量，使后续增量路径保持 O(活跃) 而非退回 O(end)
                for track_idx in 0..track_count {
                    let notes = doc.track_notes(track_idx);
                    if notes.is_empty() {
                        continue;
                    }
                    let end = notes.partition_point(tick_u32.wrapping_add(1));
                    self.playback_scan_state.scan_idx[track_idx] = end;
                }
            } else {
                // 回退：超大工程（超阈值未建索引）走原 O(end) 线性扫描
                self.playback_scan_state = PlaybackScanState {
                    last_tick: tick,
                    scan_idx: vec![0; track_count],
                    active_notes: Vec::new(),
                    doc_addr: Some(current_doc_addr),
                    key_color_index: None,
                };

                for track_idx in 0..track_count {
                    let notes = doc.track_notes(track_idx);
                    if notes.is_empty() {
                        continue;
                    }
                    let color = onion_track_color(track_idx);
                    // ChunkedList::partition_point(tick+1) = 第一个 tick > tick_u32 的索引
                    // （等价于旧 `partition_point(|n| n.start_tick <= tick_u32)`）
                    let end = notes.partition_point(tick_u32.wrapping_add(1));
                    self.playback_scan_state.scan_idx[track_idx] = end;
                    // 仅扫描 [start, end) 区间——新进入活跃的音符
                    for n in notes.iter().take(end) {
                        if n.end_tick() > tick_u32 {
                            let offset = (n.key as usize) * 4;
                            self.playback_scan_state.active_notes.push((
                                n.end_tick(),
                                offset,
                                color,
                            ));
                        }
                    }
                }
            }
        } else {
            // 增量扫描：从上次位置继续，把新进入活跃的音符 push 进 active_notes
            // 注意：scan_idx 长度可能 < track_count（首次扫描前未初始化），用 max 兜底
            let Some(doc) = self.editor_state.data.document.as_ref() else {
                return;
            };

            if self.playback_scan_state.scan_idx.len() < track_count {
                self.playback_scan_state.scan_idx.resize(track_count, 0);
            }

            for track_idx in 0..track_count {
                let notes = doc.track_notes(track_idx);
                if notes.is_empty() {
                    continue;
                }
                let color = onion_track_color(track_idx);
                let start = self.playback_scan_state.scan_idx[track_idx];
                // 等价于旧 `partition_point(|n| n.start_tick <= tick_u32)`
                let end = notes.partition_point(tick_u32.wrapping_add(1));
                self.playback_scan_state.scan_idx[track_idx] = end;
                // 仅扫描 [start, end) 区间——新进入活跃的音符
                for n in notes.iter().skip(start).take(end.saturating_sub(start)) {
                    if n.end_tick() > tick_u32 {
                        let offset = (n.key as usize) * 4;
                        self.playback_scan_state
                            .active_notes
                            .push((n.end_tick(), offset, color));
                    }
                }
            }

            // 清理已结束的音符（活跃音符数通常 < 几百，retain O(几百)）
            self.playback_scan_state
                .active_notes
                .retain(|(end_tick, _, _)| *end_tick > tick_u32);
        }

        self.playback_scan_state.last_tick = tick;

        // 用活跃音符集合涂色——遍历量 O(活跃音符数)，与已播放音符总数无关
        let mut new_colors = [0u8; 1024];
        for (_, offset, color) in &self.playback_scan_state.active_notes {
            let offset = *offset;
            new_colors[offset..offset + 4].copy_from_slice(color);
        }
        self.playback_key_colors = new_colors;
    }

    /// 为播放键色构建跨所有轨的空间索引（按 doc 缓存）
    ///
    /// 全量重建 `update_playback_key_colors` 原走 O(end) 线性扫描（end 随播放时间线性
    /// 增长）。改用本索引后，全量重建经 `update_query(tick, tick, 0, 255, _)` 直接取得
    /// 在 `tick` 活跃的音符，复杂度降为 O(活跃音符数)，与已播放音符总量无关。
    ///
    /// 仅在 `doc_changed` 时调用一次；音符总量超阈值则置 `None`，由调用方回退线性扫描。
    ///
    /// # 参数
    /// - `current_doc_addr`：当前文档地址，用于更新 `doc_addr` 缓存，避免下一帧误判文档切换
    fn rebuild_playback_key_index(&mut self, track_count: usize, current_doc_addr: usize) {
        puffin::profile_scope!("rebuild_playback_key_index");
        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return;
        };
        let mut note_refs: Vec<NoteRef> = Vec::new();
        let mut track_of: Vec<u16> = Vec::new();
        let mut key_of: Vec<u8> = Vec::new();
        let mut end_of: Vec<u32> = Vec::new();

        // 边遍历边构建；超过阈值立即中止并回退线性扫描（避免超大工程 O(N log N) 建树）
        let mut aborted = false;
        for track_idx in 0..track_count {
            let notes = doc.track_notes(track_idx);
            for n in notes.iter() {
                if note_refs.len() >= PLAYBACK_KEY_INDEX_MAX_NOTES {
                    aborted = true;
                    break;
                }
                let gi = note_refs.len();
                let start = n.start_tick;
                let end = n.end_tick();
                note_refs.push(NoteRef {
                    tick: start as f32,
                    key: n.key as u16,
                    length: (end - start) as f32,
                    index: gi,
                });
                track_of.push(track_idx as u16);
                key_of.push(n.key);
                end_of.push(end);
            }
            if aborted {
                break;
            }
        }

        self.playback_scan_state.doc_addr = Some(current_doc_addr);

        if aborted {
            self.playback_scan_state.key_color_index = None;
            return;
        }

        let index = NoteSpatialIndex::from_note_refs(&note_refs);
        self.playback_scan_state.key_color_index = Some(PlaybackKeyIndex {
            index,
            track_of,
            key_of,
            end_of,
        });
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
        // 保留 key_color_index：它仅依赖文档音符，与播放进度无关，
        // 停止/重启不应使其失效（否则重启首帧会退回 O(end) 线性扫描）。
        self.playback_scan_state.last_tick = 0.0;
        self.playback_scan_state.scan_idx.clear();
        self.playback_scan_state.active_notes.clear();
    }
}
