//! 播放键色增量扫描
//!
//! 包含：`PlaybackScanState` 缓存结构 + `update_playback_key_colors` 方法。
//!
//! 拆分原因：`editor_impl.rs` 接近 400 行限制，按职责拆分。

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
}

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
    /// # 性能策略（增量扫描）
    ///
    /// 朴素实现遍历 `[0, end)` 区间所有音符，复杂度 O(end)，其中 `end` 随播放时间
    /// 线性增长——百万级音符的 MIDI 播放 6 分钟后，每帧扫描量可达千万级。
    ///
    /// 本方法维护 `PlaybackScanState` 缓存上次扫描位置和当前活跃音符集合：
    /// - 正常播放：增量扫描新进入的音符 + retain 清理已结束音符，每帧 O(活跃音符数)
    /// - seek / 循环回绕 / 文档切换：触发全量重建
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            // 停止时重置扫描状态，下次播放从头开始
            self.playback_scan_state = PlaybackScanState::default();
            return;
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return;
        };

        let tick = self.playback_position;
        let tick_u32 = tick as u32;
        let track_count = doc.track_count();

        // 检测 MIDI 文档切换：缓存地址变化即视为新文档
        let current_doc_addr = doc as *const MidiDocument as usize;
        let doc_changed = self.playback_scan_state.doc_addr != Some(current_doc_addr);

        // 检测 tick 跳跃：回退或大幅前跳都触发全量重建
        let last_tick = self.playback_scan_state.last_tick;
        let need_full_rebuild =
            doc_changed || tick < last_tick || (tick - last_tick) > SEEK_THRESHOLD_TICKS;

        if need_full_rebuild {
            // 全量重建：scan_idx 清零，active_notes 清空，从 0 开始扫描到 end
            self.playback_scan_state = PlaybackScanState {
                last_tick: tick,
                scan_idx: vec![0; track_count],
                active_notes: Vec::new(),
                doc_addr: Some(current_doc_addr),
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
                for n in notes.iter().take(end) {
                    if n.end_tick() > tick_u32 {
                        let offset = (n.key as usize) * 4;
                        self.playback_scan_state
                            .active_notes
                            .push((n.end_tick(), offset, color));
                    }
                }
            }
        } else {
            // 增量扫描：从上次位置继续，把新进入活跃的音符 push 进 active_notes
            // 注意：scan_idx 长度可能 < track_count（首次扫描前未初始化），用 max 兜底
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
        self.playback_scan_state = PlaybackScanState::default();
    }
}
