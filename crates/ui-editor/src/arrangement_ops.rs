//! 工程走带视图音符操作（跨音轨）
//!
//! 提供 arrange_move_notes / arrange_erase / arrange_razor 三个操作，
//! 直接修改 EditorData::track_notes，并在当前音轨受影响时同步 data.notes。

use std::collections::{HashMap, HashSet};

use super::Editor;
use crate::note::Note;

type ClipboardNoteEntry = (u16, f32, u16, f32, u8, u8);

impl Editor {
    /// 移动工程走带选择区内的音符。
    ///
    /// 支持跨音轨移动（delta_tracks != 0）。移动后调用方需自行同步选择矩形。
    /// 返回实际移动的音符数。
    pub fn arrange_move_notes(&mut self, delta_ticks: i64, delta_tracks: i32) -> usize {
        if self.editor_state.data.arrange_selection.is_empty()
            || (delta_ticks == 0 && delta_tracks == 0)
        {
            return 0;
        }

        let selection = self.editor_state.data.arrange_selection.clone();
        let mut indices_by_source: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut moved_by_dest: HashMap<usize, Vec<Note>> = HashMap::new();

        {
            let data = &self.editor_state.data;
            for (&track_idx, notes) in &data.track_notes {
                for (i, note) in notes.iter().enumerate() {
                    if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                        let dest_track = (track_idx as i32 + delta_tracks).max(0) as usize;
                        let new_tick = (note.tick as f64 + delta_ticks as f64).max(0.0) as f32;
                        let mut moved = note.clone();
                        moved.tick = new_tick;
                        indices_by_source.entry(track_idx).or_default().insert(i);
                        moved_by_dest.entry(dest_track).or_default().push(moved);
                    }
                }
            }
        }

        if indices_by_source.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut moved_count = 0usize;

        {
            let data = &mut self.editor_state.data;
            for (source_track, indices) in indices_by_source {
                if source_track == current_track {
                    current_track_touched = true;
                }
                if let Some(notes) = data.track_notes.get_mut(&source_track) {
                    let before = notes.len();
                    let mut idx = 0usize;
                    notes.retain(|_| {
                        let keep = !indices.contains(&idx);
                        idx += 1;
                        keep
                    });
                    moved_count += before - notes.len();
                }
            }

            for (dest_track, notes_to_add) in moved_by_dest {
                if dest_track == current_track {
                    current_track_touched = true;
                }
                let track_entry = data.track_notes.entry(dest_track).or_default();
                for note in notes_to_add {
                    track_entry.push_back(note);
                }
            }
        }

        if moved_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 移动 {} 个音符 (delta_ticks={}, delta_tracks={})",
            moved_count,
            delta_ticks,
            delta_tracks
        );
        moved_count
    }

    /// 擦除工程走带矩形范围内的音符。
    ///
    /// 返回实际删除的音符数。
    pub fn arrange_erase(
        &mut self,
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    ) -> usize {
        if tick_start >= tick_end || track_lo > track_hi {
            return 0;
        }

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut tracks_to_clean: Vec<usize> = Vec::new();

        {
            let data = &self.editor_state.data;
            for track_idx in track_lo..=track_hi {
                if let Some(notes) = data.track_notes.get(&track_idx) {
                    let has_any = notes
                        .iter()
                        .any(|note| note_in_rect(note, tick_start, tick_end));
                    if has_any {
                        tracks_to_clean.push(track_idx);
                        if track_idx == current_track {
                            current_track_touched = true;
                        }
                    }
                }
            }
        }

        if tracks_to_clean.is_empty() {
            return 0;
        }

        self.push_history();

        let mut deleted_count = 0usize;
        {
            let data = &mut self.editor_state.data;
            for track_idx in tracks_to_clean {
                if let Some(notes) = data.track_notes.get_mut(&track_idx) {
                    let before = notes.len();
                    notes.retain(|note| !note_in_rect(note, tick_start, tick_end));
                    deleted_count += before - notes.len();
                }
            }
        }

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 擦除 {} 个音符 (tick {}..{}, track {}..={})",
            deleted_count,
            tick_start,
            tick_end,
            track_lo,
            track_hi
        );
        deleted_count
    }

    /// 在指定 tick/音轨处分割音符（Razor 工具）。
    ///
    /// 返回实际分割的音符数。
    pub fn arrange_razor(&mut self, tick: f64, track: usize) -> usize {
        let tick_f = tick as f32;

        let indices_to_split: Vec<usize> = {
            let data = &self.editor_state.data;
            let Some(notes) = data.track_notes.get(&track) else {
                return 0;
            };
            notes
                .iter()
                .enumerate()
                .filter_map(|(i, note)| {
                    if note.tick < tick_f && note.tick + note.length > tick_f {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect()
        };

        if indices_to_split.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;
        let mut split_count = 0usize;

        {
            let data = &mut self.editor_state.data;
            if let Some(notes) = data.track_notes.get_mut(&track) {
                // 从后往前分割，避免索引漂移
                for idx in indices_to_split.into_iter().rev() {
                    if let Some(note) = notes.get(idx).cloned() {
                        let left = Note::from_raw(
                            note.tick,
                            note.key,
                            tick_f - note.tick,
                            note.velocity,
                            note.channel,
                        );
                        let right = Note::from_raw(
                            tick_f,
                            note.key,
                            note.tick + note.length - tick_f,
                            note.velocity,
                            note.channel,
                        );
                        notes.remove(idx);
                        notes.insert(idx, right);
                        notes.insert(idx, left);
                        split_count += 1;
                    }
                }
            }
        }

        if split_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 分割 {} 个音符 (tick={}, track={})",
            split_count,
            tick,
            track
        );
        split_count
    }

    /// 获取当前工程走带选择范围内的音符列表。
    ///
    /// 返回 `(tick_start, tick_end, track, key)`，用于 ghost 预览。
    pub fn arrangement_selected_notes(&self) -> Vec<(f64, f64, usize, u8)> {
        let data = &self.editor_state.data;
        let selection = &data.arrange_selection;
        if selection.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        for (&track_idx, notes) in &data.track_notes {
            for note in notes {
                if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                    result.push((
                        note.tick as f64,
                        (note.tick + note.length) as f64,
                        track_idx,
                        note.key as u8,
                    ));
                }
            }
        }
        result
    }

    /// 在工程走带指定音轨 tick 处添加一个音符。
    ///
    /// 返回是否实际添加。
    pub fn arrange_add_note(
        &mut self,
        track_count: usize,
        track: usize,
        tick: f64,
        duration: f64,
        key: u8,
        velocity: u8,
    ) -> bool {
        if tick < 0.0 || duration <= 0.0 || track >= track_count {
            return false;
        }

        let tick_f = tick as f32;
        let length_f = duration as f32;
        let key_u16 = key as u16;
        let note = Note::from_raw(tick_f, key_u16, length_f, velocity, 0);

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;

        {
            let data = &mut self.editor_state.data;
            let track_entry = data.track_notes.entry(track).or_default();
            track_entry.push_back(note);
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 添加音符 (tick={}, duration={}, track={}, key={}, velocity={})",
            tick,
            duration,
            track,
            key,
            velocity
        );
        true
    }

    /// 复制工程走带选中音符到系统剪贴板（JSON 格式）。
    ///
    /// 使用与钢琴卷帘相同的剪贴板格式，额外包含 origin_track。
    /// 返回是否有音符被复制。
    pub fn arrange_copy_selected_notes(&self) -> bool {
        let data = &self.editor_state.data;
        let selection = &data.arrange_selection;
        if selection.is_empty() {
            return false;
        }

        let mut all_notes: Vec<(usize, &Note)> = Vec::new();
        for (&track_idx, notes) in &data.track_notes {
            for note in notes {
                if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                    all_notes.push((track_idx, note));
                }
            }
        }

        if all_notes.is_empty() {
            return false;
        }

        let origin_tick = all_notes
            .iter()
            .map(|(_, note)| note.tick)
            .fold(f32::INFINITY, f32::min);
        let origin_key = all_notes
            .iter()
            .map(|(_, note)| note.key)
            .min()
            .unwrap_or(0);
        let origin_track = all_notes.iter().map(|(track, _)| *track).min().unwrap_or(0);

        let note_count = all_notes.len();
        let payload = serde_json::json!({
            "lumino": lumino_ui_constants::editor::CLIPBOARD_FORMAT,
            "version": lumino_ui_constants::editor::CLIPBOARD_VERSION,
            "type": "arrangement",
            "origin_tick": origin_tick,
            "origin_key": origin_key,
            "origin_track": origin_track,
            "notes": all_notes.into_iter().map(|(track, note)| serde_json::json!({
                "tick": note.tick - origin_tick,
                "key": note.key - origin_key,
                "length": note.length,
                "velocity": note.velocity,
                "channel": note.channel,
                "track": track - origin_track,
            })).collect::<Vec<_>>(),
        });

        let mut clipboard = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(e) => {
                tracing::error!("Arrangement: 创建剪贴板失败: {}", e);
                return false;
            }
        };
        match clipboard.set_text(payload.to_string()) {
            Ok(()) => {
                tracing::info!("Arrangement: 已复制 {} 个音符", note_count);
                true
            }
            Err(e) => {
                tracing::error!("Arrangement: 复制到剪贴板失败: {}", e);
                false
            }
        }
    }

    /// 从剪贴板粘贴音符到工程走带视图。
    ///
    /// 粘贴位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - 音轨以选中区域的最小音轨为锚点，若选择为空则使用当前音轨
    /// - KEY 保持与被复制音符相同（不改变 KEY 位置）
    ///
    /// 返回是否有音符被粘贴。
    pub fn arrange_paste_notes_from_clipboard(&mut self) -> bool {
        let Some((origin_key, origin_track, notes_value)) = self.read_arrangement_clipboard_json()
        else {
            return false;
        };

        let Some((anchor_tick, anchor_track, pasted)) =
            self.parse_arrangement_clipboard_notes(origin_key, origin_track, &notes_value)
        else {
            return false;
        };

        if pasted.is_empty() {
            return false;
        }

        self.push_history();

        let _track_count = {
            let data = &self.editor_state.data;
            data.track_notes.len().max(1)
        };
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut inserted_count = 0usize;

        for (track_offset, tick_offset, key_offset, length, velocity, channel) in &pasted {
            let dest_track = (anchor_track as i32 + *track_offset as i32).max(0) as usize;
            let note_tick = (anchor_tick + tick_offset).max(0.0);
            let note_key = origin_key.saturating_add(*key_offset).min(127);
            let note = Note::from_raw(note_tick, note_key, *length, *velocity, *channel);

            let data = &mut self.editor_state.data;
            let track_entry = data.track_notes.entry(dest_track).or_default();
            track_entry.push_back(note);
            if dest_track == current_track {
                current_track_touched = true;
            }
            inserted_count += 1;
        }

        if inserted_count == 0 {
            self.editor_state.data.discard_last_history();
            return false;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 已粘贴 {} 个音符 (anchor_tick={}, anchor_track={})",
            inserted_count,
            anchor_tick,
            anchor_track
        );
        true
    }

    /// 从剪贴板读取并解析走带视图专用的 JSON 数据。
    fn read_arrangement_clipboard_json(&self) -> Option<(u16, usize, Vec<serde_json::Value>)> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let text = clipboard.get_text().ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;

        let clipboard_type = value.get("type").and_then(|t| t.as_str());
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        let origin_track = value.get("origin_track")?.as_u64()? as usize;
        let notes = value.get("notes")?.as_array()?.to_vec();

        if clipboard_type == Some("arrangement") {
            Some((origin_key, origin_track, notes))
        } else {
            tracing::warn!(
                "Arrangement: 剪贴板数据不是走带格式 (type={:?})",
                clipboard_type
            );
            None
        }
    }

    /// 从走带剪贴板 JSON 解析锚点坐标和音符列表。
    ///
    /// 粘贴位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - 音轨以选中区域的最小音轨为锚点，若选择为空则使用当前音轨
    fn parse_arrangement_clipboard_notes(
        &self,
        _origin_key: u16,
        _origin_track: usize,
        notes_value: &[serde_json::Value],
    ) -> Option<(f32, usize, Vec<ClipboardNoteEntry>)> {
        let anchor_tick = self.snap_tick(self.playback_position);

        let data = &self.editor_state.data;
        let selection = &data.arrange_selection;
        let anchor_track = if selection.is_empty() {
            data.current_track
        } else {
            let mut min_track = usize::MAX;
            for rect in &selection.rects {
                if (rect.4 as usize) < min_track {
                    min_track = rect.4 as usize;
                }
            }
            if min_track == usize::MAX {
                data.current_track
            } else {
                min_track
            }
        };

        let max_track_count = data.track_notes.len().max(1);
        let mut pasted: Vec<ClipboardNoteEntry> = Vec::with_capacity(notes_value.len());

        for item in notes_value {
            let tick_offset = item.get("tick")?.as_f64()? as f32;
            let key_offset = item.get("key")?.as_u64()? as u16;
            let length = item.get("length")?.as_f64()? as f32;
            let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
            let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
            let track_offset = item.get("track").and_then(|t| t.as_u64()).unwrap_or(0) as u16;

            let dest_track = (anchor_track as i32 + track_offset as i32).max(0) as usize;
            if dest_track >= max_track_count {
                continue;
            }

            pasted.push((
                track_offset,
                tick_offset,
                key_offset,
                length,
                velocity,
                channel,
            ));
        }

        Some((anchor_tick, anchor_track, pasted))
    }

    /// 删除工程走带选择区内的所有音符。
    ///
    /// 返回实际删除的音符数。
    pub fn arrange_delete_selected_notes(&mut self) -> usize {
        let data = &self.editor_state.data;
        let selection = &data.arrange_selection;
        if selection.is_empty() {
            return 0;
        }

        let mut indices_by_track: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (&track_idx, notes) in &data.track_notes {
            for (i, note) in notes.iter().enumerate() {
                if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                    indices_by_track.entry(track_idx).or_default().push(i);
                }
            }
        }

        if indices_by_track.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut deleted_count = 0usize;

        {
            let data = &mut self.editor_state.data;
            for (track_idx, mut indices) in indices_by_track {
                if track_idx == current_track {
                    current_track_touched = true;
                }
                indices.sort_unstable_by(|a, b| b.cmp(a));
                if let Some(notes) = data.track_notes.get_mut(&track_idx) {
                    for idx in indices {
                        notes.remove(idx);
                        deleted_count += 1;
                    }
                }
            }
        }

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!("Arrangement: 删除 {} 个音符", deleted_count);
        deleted_count
    }

    /// 剪切工程走带选中音符（复制 + 删除）。
    ///
    /// 返回实际剪切的音符数（删除的音符数）。
    pub fn arrange_cut_selected_notes(&mut self) -> usize {
        let copied = self.arrange_copy_selected_notes();
        if !copied {
            return 0;
        }
        self.arrange_delete_selected_notes()
    }

    /// 工程走带操作后，若当前音轨受影响则同步 data.notes 与 NoteStore。
    fn sync_current_track_after_arrange_op(&mut self, touched: bool) {
        if !touched {
            return;
        }
        let data = &mut self.editor_state.data;
        data.notes = data
            .track_notes
            .get(&data.current_track)
            .cloned()
            .unwrap_or_default();
        if data.is_note_store_enabled() {
            data.sync_note_store();
        }
        self.mark_notes_changed();
    }
}

/// 判断音符是否与擦除矩形相交（tick 半开区间 [tick_start, tick_end)）。
fn note_in_rect(note: &Note, tick_start: f64, tick_end: f64) -> bool {
    let ne = note.tick + note.length;
    note.tick < tick_end as f32 && ne > tick_start as f32
}
