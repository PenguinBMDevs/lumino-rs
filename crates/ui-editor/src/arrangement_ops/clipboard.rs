//! 工程走带剪贴板操作（复制/粘贴/剪切）
//!
//! 使用与钢琴卷帘相同的 JSON 剪贴板格式，额外包含 origin_track。

use super::helpers::ClipboardNoteEntry;
use super::helpers::note_event_to_note;
use super::Editor;
use crate::note::Note;

impl Editor {
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

        let all_notes = self.collect_selected_notes_for_clipboard();

        if all_notes.is_empty() {
            return false;
        }

        self.write_arrangement_clipboard(all_notes)
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

        let (inserted_count, current_track_touched) =
            self.apply_paste_internal(anchor_tick, anchor_track, origin_key, &pasted);

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

    // ── 私有辅助方法 ─────────────────────────────────────

    /// 构建并写入剪贴板 JSON。
    fn write_arrangement_clipboard(&self, all_notes: Vec<(usize, Note)>) -> bool {
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

    /// 从 track_notes 和 MidiDocument 收集所有选中音符。
    fn collect_selected_notes_for_clipboard(&self) -> Vec<(usize, Note)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
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
                    if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key) {
                        let note = note_event_to_note(note_event);
                        all_notes.push((track_idx, note));
                    }
                }
            }
        }

        all_notes
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

    /// 执行粘贴：将剪贴板音符插入目标音轨。
    /// 返回 (inserted_count, current_track_touched)。
    fn apply_paste_internal(
        &mut self,
        anchor_tick: f32,
        anchor_track: usize,
        origin_key: u16,
        pasted: &[ClipboardNoteEntry],
    ) -> (usize, bool) {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut inserted_count = 0usize;

        for (track_offset, tick_offset, key_offset, length, velocity, channel) in pasted {
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

        (inserted_count, current_track_touched)
    }

    /// 计算粘贴锚点音轨：优先使用选区最小音轨，为空则用当前音轨。
    fn compute_anchor_track(&self) -> usize {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        if selection.is_empty() {
            return editor_data.current_track;
        }
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

        let anchor_track = self.compute_anchor_track();

        let editor_data = &self.editor_state.data;
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
}
