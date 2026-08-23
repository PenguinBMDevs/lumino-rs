//! 选中集合操作：insert / remove / clear / assign
//!
//! `selected_bounds` 缓存随选中集合变更增量维护，避免后续
//! `get_selection_box_bounds()` 走 O(N) 全量扫描。

use std::collections::HashSet;

use super::Editor;
use crate::Note;
use crate::RemoteSelectionSet;

impl Editor {
    /// 向选中集合添加音符，并增量更新选择框边界。
    /// 避免后续 get_selection_box_bounds() 做 O(N) 全量扫描。
    ///
    /// 如果 `selection_bitset` 已激活，直接设置对应位（O(1)），无需回退到 HashSet。
    pub fn selection_insert(&mut self, index: usize) {
        // `selection_bitset` 路径：直接设置位，O(1) 零分配
        if let Some(ref mut bs) = self.editor_state.interaction.selection_bitset {
            if bs.get(index) {
                return; // 音符已选中
            }
            bs.set(index);
        } else {
            let inserted = self.editor_state.interaction.selected_notes.insert(index);
            if !inserted {
                return; // 音符已选中，不需要更新边界
            }
        }
        if let Some(n) = self.editor_state.data.get_note_view(index) {
            let mut bounds = self.selected_bounds.get();
            match &mut bounds {
                Some((min_t, max_te, max_k, min_k)) => {
                    *min_t = min_t.min(n.tick);
                    *max_te = max_te.max(n.tick + n.length);
                    *max_k = u16::max(*max_k, n.key);
                    *min_k = u16::min(*min_k, n.key);
                }
                None => {
                    bounds = Some((n.tick, n.tick + n.length, n.key, n.key));
                }
            }
            self.selected_bounds.set(bounds);
        }
    }

    /// 从选中集合移除音符，如果被移除的音符在边界上则失效缓存。
    ///
    /// 如果 `selection_bitset` 已激活，直接清除对应位（O(1)），无需回退到 HashSet。
    pub fn selection_remove(&mut self, index: &usize) -> bool {
        // `selection_bitset` 路径：直接清除位，O(1) 零分配
        if let Some(ref mut bs) = self.editor_state.interaction.selection_bitset {
            if !bs.get(*index) {
                return false;
            }
            bs.clear_at(*index);
        } else {
            let removed = self.editor_state.interaction.selected_notes.remove(index);
            if !removed {
                return false;
            }
        }
        // 检查被移除的音符是否在边界上，若在则缓存失效
        if let Some(bounds) = self.selected_bounds.get()
            && let Some(n) = self.editor_state.data.get_note_view(*index)
        {
            let at_boundary = n.tick == bounds.0
                || n.tick + n.length == bounds.1
                || n.key == bounds.2
                || n.key == bounds.3;
            if at_boundary {
                self.selected_bounds.set(None);
            }
        }
        true
    }

    /// 清空选中集合，并清除选择框边界缓存。
    ///
    /// 同时清除 `selection_bitset` 和 `selected_notes`。
    pub fn selection_clear(&mut self) {
        self.editor_state.interaction.selection_bitset = None;
        self.editor_state.interaction.selected_notes.clear();
        self.selected_bounds.set(None);
    }

    /// 替换选中集合，并重建选择框边界缓存。
    ///
    /// 清空 `selection_bitset`（全量替换不走位向量路径）。
    /// 性能优化：直接扫描新集合重建 selected_bounds，避免首次调用
    /// `get_selection_box_bounds()` 时做 O(N) 兜底扫描。
    /// 对超大选中集（1600W）的首次调用，将 O(N) 从 `get_selection_box_bounds`
    /// 和 `selection_box::draw` 各一次合并为一次，消除双重扫描。
    pub fn selection_assign(&mut self, new_set: HashSet<usize>) {
        // 全量替换时清除 bitset
        self.editor_state.interaction.selection_bitset = None;
        let data = &self.editor_state.data;
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        let mut any = false;
        for &i in new_set.iter() {
            if let Some(n) = data.get_note_view(i) {
                any = true;
                min_t = min_t.min(n.tick);
                max_te = max_te.max(n.tick + n.length);
                max_k = max_k.max(n.key);
                min_k = min_k.min(n.key);
            }
        }
        self.editor_state.interaction.selected_notes = new_set;
        self.selected_bounds.set(if any {
            Some((min_t, max_te, max_k, min_k))
        } else {
            None
        });
    }

    /// 按参数全等（tick/key/length/velocity/channel）匹配并选中音符
    ///
    /// 用于批量插入后重新定位选中：插入会位移既有音符的全局索引，旧索引
    /// 失效，改按音符参数精确匹配新索引。与 hit_test 的 ChunkedList 窗口
    /// 扫描同机制（O(log N + K)）。
    ///
    /// **匹配全部命中**（不 break）：副本 key 被 clamp 后可能与原件参数
    /// 完全相同（重叠场景），此时原件与副本须全部选中——两者都合法。
    pub(crate) fn select_notes_by_params(&mut self, notes: &[Note]) {
        let track_notes = self.editor_state.data.current_track_notes();
        let mut indices: Vec<usize> = Vec::with_capacity(notes.len());
        for note in notes {
            let tick_u32 = note.tick.max(0.0) as u32;
            let (lo, hi) = track_notes.window_range(tick_u32, tick_u32 + 1, 0);
            for (i, ev) in track_notes.iter_window(lo, hi) {
                if ev.start_tick as f32 == note.tick
                    && ev.key == note.key as u8
                    && (ev.end_tick - ev.start_tick) as f32 == note.length
                    && ev.velocity == note.velocity
                    && ev.channel == note.channel
                {
                    indices.push(i);
                }
            }
        }
        for i in indices {
            self.selection_insert(i);
        }
    }
}

/// 当前毫秒时间戳（用于选择时间戳 / first-writer-wins 冲突判定）
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl Editor {
    /// 计算当前选中音符的指纹列表 `(track_index, tick, key, length)`
    pub fn selected_fingerprints(&self) -> Vec<(usize, f32, u16, f32)> {
        let track = self.editor_state.data.current_track;
        self.get_selected_indices()
            .into_iter()
            .filter_map(|i| {
                self.editor_state
                    .data
                    .get_note_view(i)
                    .map(|n| (track, n.tick, n.key, n.length))
            })
            .collect()
    }

    /// 应用远端用户的选择更新（来自协作 `Selection` 事件）
    ///
    /// - `active == false`：移除该用户的远端选择（取消/编辑已提交）。
    /// - `active == true`：记录指纹与时间戳，供高亮渲染与冲突判定使用。
    pub fn apply_remote_selection(&mut self, user_id: &str, selection_json: &str, color: &str) {
        let parsed: serde_json::Value = match serde_json::from_str(selection_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("协作: 解析远端选择失败: {}", e);
                return;
            }
        };
        let active = parsed
            .get("active")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !active {
            self.remote_selections.remove(user_id);
            self.grid_cache.clear();
            return;
        }
        let timestamp = parsed
            .get("timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mut fingerprints = Vec::new();
        if let Some(arr) = parsed.get("fingerprints").and_then(|v| v.as_array()) {
            for f in arr {
                if let (Some(t), Some(ti), Some(k), Some(l)) = (
                    f.get(0).and_then(|v| v.as_f64()),
                    f.get(1).and_then(|v| v.as_f64()),
                    f.get(2).and_then(|v| v.as_f64()),
                    f.get(3).and_then(|v| v.as_f64()),
                ) {
                    fingerprints.push((t as usize, ti as f32, k as u16, l as f32));
                }
            }
        }
        self.remote_selections.insert(
            user_id.to_string(),
            RemoteSelectionSet {
                timestamp,
                fingerprints,
                color: color.to_string(),
            },
        );
        self.grid_cache.clear();
    }

    /// 移除远端用户的选择（用户离开等）
    pub fn remove_remote_selection(&mut self, user_id: &str) {
        self.remote_selections.remove(user_id);
        self.grid_cache.clear();
    }

    /// first-writer-wins 冲突判定
    ///
    /// 若 `notes` 中任一音符与某个**更早**（`timestamp < local_timestamp`）的
    /// 远端选择指纹重叠（同轨、同键、tick 区间相交），返回 `true`：
    /// 表示远端已先占该音符，本地编辑应让行（远端优先）。
    pub fn is_locked_by_other(
        &self,
        local_timestamp: u64,
        notes: &[(usize, f32, u16, f32)],
    ) -> bool {
        for sel in self.remote_selections.values() {
            if sel.timestamp >= local_timestamp {
                continue; // 远端不更早，不锁定
            }
            for (rtrack, rtick, rkey, rlen) in &sel.fingerprints {
                for (track, tick, key, len) in notes {
                    if *rtrack == *track
                        && *rkey == *key
                        && *tick < *rtick + *rlen
                        && *tick + *len > *rtick
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 用当前本地选择状态判定是否被远端更早选择锁定
    pub(crate) fn local_selection_is_locked(&self) -> bool {
        match self.local_selection_timestamp {
            Some(ts) => self.is_locked_by_other(ts, &self.local_selection_fingerprints),
            None => false,
        }
    }

    /// 广播本地选择变更（供协作客户端转发到其他用户）
    ///
    /// - `active == true`：写入选中指纹 + 时间戳，并作为本地选择会话起点。
    /// - `active == false`：清空本地选择会话并通知对端选择已结束。
    pub fn emit_local_selection_changed(&mut self, active: bool) {
        let timestamp = if active {
            now_ms()
        } else {
            self.local_selection_timestamp.unwrap_or(0)
        };
        if active {
            let fps = self.selected_fingerprints();
            self.local_selection_timestamp = Some(timestamp);
            self.local_selection_fingerprints = fps.clone();
            let json_fps: Vec<[f64; 4]> = fps
                .iter()
                .map(|(t, ti, k, l)| [*t as f64, *ti as f64, *k as f64, *l as f64])
                .collect();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_selection_changed(
                    true, timestamp, json_fps,
                ),
            ));
        } else {
            self.local_selection_timestamp = None;
            self.local_selection_fingerprints.clear();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_selection_changed(
                    false,
                    timestamp,
                    Vec::new(),
                ),
            ));
        }
    }
}
