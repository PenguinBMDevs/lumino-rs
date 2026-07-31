//! 工程走带视图音符操作（跨音轨）
//!
//! 提供 arrange_move_notes / arrange_erase / arrange_razor 三个操作，
//! 直接修改 EditorData::track_notes，并在当前音轨受影响时同步 editor_data.notes。

use std::collections::{HashMap, HashSet};

use super::Editor;
use crate::note::Note;
use lumino_midi_loader::NoteEvent;

type ClipboardNoteEntry = (u16, f32, u16, f32, u8, u8);

/// 将 MIDI 模型的 NoteEvent 转换为编辑器 Note。
fn note_event_to_note(event: &NoteEvent) -> Note {
    Note::from_raw(
        event.start_tick as f32,
        event.key as u16,
        (event.end_tick - event.start_tick) as f32,
        event.velocity,
        event.channel,
    )
}

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

        self.load_missing_tracks_from_document();
        let selection = self.editor_state.data.arrange_selection.clone();
        let mut indices_by_source: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut moved_by_dest: HashMap<usize, Vec<Note>> = HashMap::new();

        {
            let editor_data = &self.editor_state.data;
            for (&track_idx, notes) in &editor_data.track_notes {
                let visual_pos = editor_data
                    .visual_position_of(track_idx)
                    .unwrap_or(track_idx);
                for (i, note) in notes.iter().enumerate() {
                    if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                        let dest_visual = (visual_pos as i32 + delta_tracks).max(0) as usize;
                        let dest_track = editor_data
                            .track_visual_order
                            .get(dest_visual)
                            .copied()
                            .unwrap_or(dest_visual);
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
            let editor_data = &mut self.editor_state.data;
            for (source_track, indices) in indices_by_source {
                if source_track == current_track {
                    current_track_touched = true;
                }
                if let Some(notes) = editor_data.track_notes.get_mut(&source_track) {
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
                let track_entry = editor_data.track_notes.entry(dest_track).or_default();
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
            let editor_data = &self.editor_state.data;
            for track_idx in track_lo..=track_hi {
                if let Some(notes) = editor_data.track_notes.get(&track_idx) {
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
            let editor_data = &mut self.editor_state.data;
            for track_idx in tracks_to_clean {
                if let Some(notes) = editor_data.track_notes.get_mut(&track_idx) {
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
            let editor_data = &self.editor_state.data;
            let Some(notes) = editor_data.track_notes.get(&track) else {
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
            let editor_data = &mut self.editor_state.data;
            if let Some(notes) = editor_data.track_notes.get_mut(&track) {
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
    /// track 为视觉位置（侧边栏顺序），而非文档音轨索引。
    /// ghost 计算中的 dtr 是视觉空间偏移，与视觉位置相加得到正确的渲染位置。
    pub fn arrangement_selected_notes(&self) -> Vec<(f64, f64, usize, u8)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        if selection.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();

        // 1. 从 track_notes 缓存收集
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for note in notes {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    result.push((
                        note.tick as f64,
                        (note.tick + note.length) as f64,
                        visual_pos,
                        note.key as u8,
                    ));
                }
            }
        }

        // 2. 从 MidiDocument 收集未加载到 track_notes 的音轨中的音符
        if let Some(doc) = &editor_data.document {
            for track_idx in 0..doc.notes.len() {
                if editor_data.track_notes.contains_key(&track_idx) {
                    continue;
                }
                let visual_pos = editor_data
                    .visual_position_of(track_idx)
                    .unwrap_or(track_idx);
                for note_event in doc.track_notes(track_idx) {
                    if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key)
                    {
                        result.push((
                            note_event.start_tick as f64,
                            note_event.end_tick as f64,
                            visual_pos,
                            note_event.key,
                        ));
                    }
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
            let editor_data = &mut self.editor_state.data;
            let track_entry = editor_data.track_notes.entry(track).or_default();
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
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        if selection.is_empty() {
            return false;
        }

        let mut all_notes: Vec<(usize, Note)> = Vec::new();

        // 1. 从 track_notes 缓存收集
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for note in notes {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    all_notes.push((track_idx, note.clone()));
                }
            }
        }

        // 2. 从 MidiDocument 收集未加载到 track_notes 的音轨中的音符
        if let Some(doc) = &editor_data.document {
            for track_idx in 0..doc.notes.len() {
                if editor_data.track_notes.contains_key(&track_idx) {
                    continue;
                }
                let visual_pos = editor_data
                    .visual_position_of(track_idx)
                    .unwrap_or(track_idx);
                for note_event in doc.track_notes(track_idx) {
                    if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key)
                    {
                        let note = note_event_to_note(note_event);
                        all_notes.push((track_idx, note));
                    }
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
            let editor_data = &self.editor_state.data;
            editor_data.track_notes.len().max(1)
        };
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut inserted_count = 0usize;

        for (track_offset, tick_offset, key_offset, length, velocity, channel) in &pasted {
            let dest_track = (anchor_track as i32 + *track_offset as i32).max(0) as usize;
            let note_tick = (anchor_tick + tick_offset).max(0.0);
            let note_key = origin_key.saturating_add(*key_offset).min(127);
            let note = Note::from_raw(note_tick, note_key, *length, *velocity, *channel);

            let editor_data = &mut self.editor_state.data;
            let track_entry = editor_data.track_notes.entry(dest_track).or_default();
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

        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let anchor_track = if selection.is_empty() {
            editor_data.current_track
        } else {
            let mut min_track = usize::MAX;
            for rect in &selection.rects {
                if (rect.4 as usize) < min_track {
                    min_track = rect.4 as usize;
                }
            }
            if min_track == usize::MAX {
                editor_data.current_track
            } else {
                min_track
            }
        };

        let max_track_count = editor_data.track_notes.len().max(1);
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
        if self.editor_state.data.arrange_selection.is_empty() {
            return 0;
        }

        self.load_missing_tracks_from_document();

        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let mut indices_by_track: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in notes.iter().enumerate() {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
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
            let editor_data = &mut self.editor_state.data;
            for (track_idx, mut indices) in indices_by_track {
                if track_idx == current_track {
                    current_track_touched = true;
                }
                indices.sort_unstable_by(|a, b| b.cmp(a));
                if let Some(notes) = editor_data.track_notes.get_mut(&track_idx) {
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

    /// 对工程走带选择区内的音符执行批量变速。
    ///
    /// 行为与钢琴卷帘 `apply_speed_change` 一致：以选中音符的最小 tick 为基准，
    /// 按 `speed_factor` 缩放 tick 和 length。支持跨音轨操作。
    /// 返回实际修改的音符数。
    pub fn arrange_apply_speed_change(&mut self, speed_factor: f32) -> usize {
        if self.editor_state.data.arrange_selection.is_empty() {
            return 0;
        }

        self.load_missing_tracks_from_document();

        let selection = self.editor_state.data.arrange_selection.clone();
        let mut track_indices: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut min_tick = f32::INFINITY;

        // 第一遍：收集所有选中音符的索引和最小 tick
        {
            let editor_data = &self.editor_state.data;
            for (&track_idx, notes) in &editor_data.track_notes {
                let visual_pos = editor_data
                    .visual_position_of(track_idx)
                    .unwrap_or(track_idx);
                for (i, note) in notes.iter().enumerate() {
                    if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                        track_indices.entry(track_idx).or_default().push(i);
                        min_tick = min_tick.min(note.tick);
                    }
                }
            }
        }

        if track_indices.is_empty() || min_tick.is_infinite() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut modified_count = 0usize;
        const MIN_LEN: f32 = 1.0;

        // 第二遍：执行变速
        {
            let editor_data = &mut self.editor_state.data;
            for (track_idx, indices) in &track_indices {
                if *track_idx == current_track {
                    current_track_touched = true;
                }
                if let Some(notes) = editor_data.track_notes.get_mut(track_idx) {
                    for &i in indices {
                        if let Some(note) = notes.get_mut(i) {
                            let nt = min_tick + (note.tick - min_tick) * speed_factor;
                            let nl = (note.length * speed_factor).max(MIN_LEN);
                            if (nt - note.tick).abs() > f32::EPSILON
                                || (nl - note.length).abs() > f32::EPSILON
                            {
                                note.tick = nt;
                                note.length = nl;
                                modified_count += 1;
                            }
                        }
                    }
                }
            }
        }

        if modified_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 变速 {} 个音符 (factor={})",
            modified_count,
            speed_factor,
        );
        modified_count
    }

    /// 工程走带操作后，若当前音轨受影响则同步 editor_data.notes 与 NoteStore。
    fn sync_current_track_after_arrange_op(&mut self, touched: bool) {
        if !touched {
            return;
        }
        let editor_data = &mut self.editor_state.data;
        editor_data.notes = editor_data
            .track_notes
            .get(&editor_data.current_track)
            .cloned()
            .unwrap_or_default();
        if editor_data.is_note_store_enabled() {
            editor_data.sync_note_store();
        }
        self.mark_notes_changed();
    }

    /// 从 MidiDocument 加载尚未被 track_notes 缓存的音轨。
    ///
    /// 加载所有音轨而非仅 selection 覆盖的音轨，因为 ArrangeSelection 存储的是
    /// 视觉音轨位置（侧边栏顺序），而 track_notes 使用文档音轨索引。在默认的
    /// ChannelGrouped 模式下两者不一致，按 selection 筛选会导致错误/缺失音轨加载。
    /// 全量加载后由主循环中的 selection.contains 做 tick/key 层面筛选。
    fn load_missing_tracks_from_document(&mut self) {
        let tracks_to_load: Vec<usize> = {
            let editor_data = &self.editor_state.data;
            let Some(doc) = &editor_data.document else {
                return;
            };
            let mut result = Vec::new();
            for track_idx in 0..doc.notes.len() {
                if !editor_data.track_notes.contains_key(&track_idx) {
                    result.push(track_idx);
                }
            }
            result
        };

        if tracks_to_load.is_empty() {
            return;
        }

        let editor_data = &mut self.editor_state.data;
        for track_idx in tracks_to_load {
            let Some(doc) = &editor_data.document else {
                continue;
            };
            let doc_notes = doc.track_notes(track_idx);
            let mut loaded: im::Vector<Note> = im::Vector::new();
            for ne in doc_notes {
                loaded.push_back(note_event_to_note(ne));
            }
            editor_data.track_notes.insert(track_idx, loaded);
        }
        editor_data.mark_track_notes_changed();
    }
}

/// 判断音符是否与擦除矩形相交（tick 半开区间 [tick_start, tick_end)）。
fn note_in_rect(note: &Note, tick_start: f64, tick_end: f64) -> bool {
    let ne = note.tick + note.length;
    note.tick < tick_end as f32 && ne > tick_start as f32
}
