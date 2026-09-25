//! 音符选择与删除（自 `notes.rs` 拆分，保持各文件 < 400 行）

use super::super::super::selection_set::SelectionSet;

use super::EditorData;
use super::editing::merge_descending_ranges;

/// 单次删除仍走「逐区间增量事件」的区间数上限。
///
/// 渲染侧每个区间 = 一次 `remove_at`（GPU 段尾搬移，O(段尾)），区间数过多时
/// 总成本 O(区间数 × 段长) 反超整段重建。超过此上限改为单轨段重建
/// （当前轨 `main_track_struct_changed`；其他轨由调用方标记走整轨 `TrackDelta`）。
/// 真实交互选区（框选/全选/多区域）区间数通常 ≤ 个位数。
pub const MAX_INCREMENTAL_REMOVE_RANGES: usize = 16;

impl EditorData {
    /// 通过索引删除单个音符（直接操作 document 当前轨）
    ///
    /// 历史快照在变更前入栈（undo 恢复变更前状态）；删除失败（索引越界）
    /// 时丢弃快照，避免产生空撤销步。
    pub fn delete_note_by_index(&mut self, index: usize) {
        self.push_history();
        if self.remove_note(self.current_track, index).is_some() {
            self.mark_current_track_changed();
        } else {
            self.discard_last_history();
        }
    }

    /// 批量删除选中音符
    ///
    /// 选中位图直接构建降序连续区间（免 O(K) 索引 Vec 与排序），经
    /// [`Self::remove_ranges_merged`] 单次批量删除（块内单遍压缩 + 单次索引重建），
    /// 并按区间下发 `RemoveAt { index, count }` 增量事件（降序），替代原每音符
    /// 一条 `RemoveAt { count: 1 }`：GPU 段内左移次数从 K（选中数）降至
    /// 「连续删除段数」，整轨框选等海量选中场景下避免 K 次整段搬移的全量级开销。
    /// 全程走 `note_delta_events` 增量通道，`note_delta_dirty` 不置位，
    /// 渲染层不触发全量兜底重建（不走全量重建）。
    pub fn delete_selected_notes(&mut self, selected: &SelectionSet) {
        if selected.is_empty() {
            return;
        }
        let ranges = super::editing::descending_ranges_from_selection(selected);
        // 历史快照必须在变更**之前**入栈（快照 = 变更前状态，undo 才能恢复）；
        // 无实际删除（索引越界等）时丢弃该条目，避免空撤销步。
        self.push_history();
        let deleted = self.remove_ranges_merged(self.current_track, &ranges);
        if deleted == 0 {
            self.discard_last_history();
            return;
        }
        self.mark_current_track_changed();
    }

    /// 批量删除指定音轨的若干索引（通用入口：排序去重 + 区间归并后委托）。
    ///
    /// 返回实际删除数。调用方负责历史入栈与变化标记（见
    /// [`Self::delete_selected_notes`] / 走带删除入口）。
    pub fn remove_notes_merged(&mut self, track_id: usize, indices: &[usize]) -> usize {
        if indices.is_empty() {
            return 0;
        }
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        sorted.dedup();
        let ranges = merge_descending_ranges(&sorted);
        self.remove_ranges_merged(track_id, &ranges)
    }

    /// 批量删除指定音轨的**降序连续区间**，并按区间记录增量删除（不置全量脏标记）。
    ///
    /// `ranges` 为 `(起始索引, 数量)`，降序、互不重叠（见
    /// [`super::editing::descending_ranges_from_selection`]）。单次调用
    /// `MidiDocument::remove_note_ranges`（块内单遍压缩 + 单次索引重建；
    /// 旧逐音符 `remove_note` 路径每次重建 O(块数) 且非尾部区间为 O(K×块长) 搬移）。
    /// 事件按轨分流：
    /// - 当前轨 → `note_delta_events`（主轨段内 `RemoveAt` 增量）
    /// - 其他轨 → `pending_track_remove_ranges`（UI 层转 `TrackRemoveRanges`
    ///   区间级增量，**替代整轨 `TrackDelta` 重建**）
    ///
    /// 同帧对同轨多次调用时，后续区间追加在原区间之后：各区间按记录顺序
    /// 依次应用（后一批索引基于前一批删除后的状态），语义正确。
    pub fn remove_ranges_merged(&mut self, track_id: usize, ranges: &[(usize, usize)]) -> usize {
        if ranges.is_empty() {
            return 0;
        }
        let deleted = match self.document.as_mut() {
            Some(doc) => doc.remove_note_ranges(track_id, ranges),
            None => 0,
        };
        if deleted == 0 {
            return 0;
        }

        // 事件区间裁剪到删除前长度（防御越界索引：实际删除数可能小于区间和）；
        // 区间过多时走整段重建，跳过 O(区间数) 事件载荷构建。
        let incremental = ranges.len() <= MAX_INCREMENTAL_REMOVE_RANGES;
        if track_id == self.current_track {
            if incremental {
                let track_len = self.track_notes(track_id).len() + deleted;
                for (index, count) in clamp_ranges(ranges, track_len) {
                    self.note_delta_events
                        .push(super::super::NoteDeltaEvent::RemoveAt { index, count });
                }
            } else {
                // 区间过多：逐区间事件会在渲染侧产生 O(区间数 × 段尾) GPU 搬移
                // （每区间一次 `remove_at`）。改为单轨段重建（整段一次，含 doc 权威内容），
                // 与批量粘贴同一机制；本方法负责清空残留段内事件（doc 为准，不会丢更新）。
                self.mark_main_track_struct_changed();
            }
        } else if incremental {
            let track_len = self.track_notes(track_id).len() + deleted;
            let event_ranges: Vec<(usize, usize)> = clamp_ranges(ranges, track_len).collect();
            if let Some(entry) = self
                .pending_track_remove_ranges
                .iter_mut()
                .find(|(t, _)| *t == track_id)
            {
                entry.1.extend(event_ranges);
            } else {
                self.pending_track_remove_ranges
                    .push((track_id, event_ranges));
            }
        }
        // else：非当前轨且区间过多 → 不排队间；调用方 `mark_track_notes_changed_for`
        // 已标记受影响轨，洋葱皮走整轨 `TrackDelta` 重建（一次上传替代 O(区间数) 搬移）。
        deleted
    }

    /// 取出并清空非当前轨待同步的区间删除（UI 层每帧消费 → `TrackRemoveRanges`）
    pub fn take_pending_track_remove_ranges(&mut self) -> Vec<(usize, Vec<(usize, usize)>)> {
        std::mem::take(&mut self.pending_track_remove_ranges)
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> SelectionSet {
        (0..self.current_track_note_count()).collect()
    }

    /// 计算选择框内的音符索引（委托到 get_notes_in_selection_box，消除重复逻辑）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> SelectionSet {
        self.get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
            .into_iter()
            .collect()
    }

    /// 获取选择框内的音符索引列表
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        let ts = start_tick.min(current_tick);
        let te = start_tick.max(current_tick);
        let km = start_key.min(current_key);
        let kx = start_key.max(current_key);
        let mut results = Vec::new();
        for (note_idx, note) in self.current_track_notes().iter().enumerate() {
            let tick = note.start_tick as f32;
            let ne = note.end_tick as f32;
            if note.key as u16 >= km && note.key as u16 <= kx && tick <= te && ne >= ts {
                results.push(note_idx);
            }
        }
        results
    }
}

/// 将降序区间裁剪到 `[0, track_len)`（防御越界索引：实际删除数可能小于区间和）
fn clamp_ranges(
    ranges: &[(usize, usize)],
    track_len: usize,
) -> impl Iterator<Item = (usize, usize)> + '_ {
    ranges.iter().copied().filter_map(move |(start, count)| {
        if start >= track_len {
            return None;
        }
        let count = count.min(track_len - start);
        (count > 0).then_some((start, count))
    })
}
