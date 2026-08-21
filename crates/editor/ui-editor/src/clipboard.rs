//! 剪贴板操作：复制、剪切、粘贴音符

use super::Editor;
use lumino_ui_core::constants::editor::{CLIPBOARD_FORMAT, CLIPBOARD_VERSION};

impl Editor {
    /// 剪切选中音符
    pub(crate) fn cut_selected_notes(&mut self) {
        if self.copy_selected_notes_to_clipboard() {
            self.delete_selected_notes();
        }
    }

    /// 复制选中音符
    pub(crate) fn copy_selected_notes(&mut self) {
        let _ = self.copy_selected_notes_to_clipboard();
    }

    /// 将选中音符复制到系统剪贴板（JSON 格式）
    pub(crate) fn copy_selected_notes_to_clipboard(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }

        let mut indices = self.get_selected_indices();
        let count = indices.len();
        indices.sort_unstable();

        // 2026-08 单一权威源：从 document 当前轨读取（NoteEvent，u32 tick）
        let notes: Vec<&lumino_midi_loader::NoteEvent> = indices
            .into_iter()
            .filter_map(|index| self.editor_state.data.current_track_notes().get(index))
            .collect();

        if notes.is_empty() {
            return false;
        }

        let origin_tick = notes
            .iter()
            .map(|note| note.start_tick as f32)
            .fold(f32::INFINITY, f32::min);
        let origin_key = notes.iter().map(|note| note.key as u16).min().unwrap_or(0);

        let payload = serde_json::json!({
            "lumino": CLIPBOARD_FORMAT,
            "version": CLIPBOARD_VERSION,
            "track": self.editor_state.data.current_track,
            "origin_tick": origin_tick,
            "origin_key": origin_key,
            "notes": notes.into_iter().map(|note| serde_json::json!({
                "tick": note.start_tick as f32 - origin_tick,
                "key": note.key as u16 - origin_key,
                "length": (note.end_tick - note.start_tick) as f32,
                "velocity": note.velocity,
                "channel": note.channel,
            })).collect::<Vec<_>>(),
        });

        let mut clipboard = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(e) => {
                tracing::error!("Editor: 创建剪贴板失败: {}", e);
                return false;
            }
        };
        match clipboard.set_text(payload.to_string()) {
            Ok(()) => {
                tracing::info!("Editor: 已复制 {} 个音符", count);
                true
            }
            Err(e) => {
                tracing::error!("Editor: 复制到剪贴板失败: {}", e);
                false
            }
        }
    }

    /// 从剪贴板粘贴音符
    pub(crate) fn paste_notes_from_clipboard(&mut self) {
        let Some((origin_key, notes_value)) = self.read_clipboard_json() else {
            return;
        };

        let Some((anchor, pasted)) = self.parse_clipboard_notes(origin_key, &notes_value) else {
            return;
        };

        if pasted.is_empty() {
            return;
        }

        self.commit_pasted_notes(anchor, pasted);
    }

    /// 从剪贴板读取并解析 JSON 数据，返回 (origin_key, notes 数组)
    fn read_clipboard_json(&self) -> Option<(u16, Vec<serde_json::Value>)> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let text = clipboard.get_text().ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        let notes = value.get("notes")?.as_array()?.to_vec();
        Some((origin_key, notes))
    }

    /// 从剪贴板 JSON 解析锚点坐标和音符列表
    ///
    /// 粘贴位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - Y 坐标（key）保持与被复制音符相同（origin_key）
    fn parse_clipboard_notes(
        &self,
        origin_key: u16,
        notes_value: &[serde_json::Value],
    ) -> Option<((f32, u16), Vec<super::Note>)> {
        let anchor = (self.snap_tick(self.playback_position), origin_key);

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let pasted: Vec<super::Note> = notes_value
            .iter()
            .filter_map(|item| {
                let tick_offset = item.get("tick")?.as_f64()?;
                let key_offset = item.get("key")?.as_u64()? as u16;
                let length = item.get("length")?.as_f64()?;
                let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
                let tick = (anchor.0 + tick_offset as f32).max(0.0);
                let key = anchor.1.saturating_add(key_offset).min(max_key);
                Some(super::Note::from_raw(
                    tick,
                    key,
                    length as f32,
                    velocity,
                    channel,
                ))
            })
            .collect();

        Some((anchor, pasted))
    }

    /// 将解析的音符提交到编辑器并选中（O(N+M) 批量归并）
    fn commit_pasted_notes(&mut self, _anchor: (f32, u16), pasted: Vec<super::Note>) {
        self.push_history();
        self.selection_clear();
        let pasted_count = pasted.len();
        // 批量归并：单次重建替代 N 次 insert，峰值仅单块 8MB
        self.editor_state.data.batch_insert_notes(&pasted);
        // 批量插入索引散布，旧的 start..start+count 连续选中在 tick 重叠时失效
        // → 按参数全等重选（与 commit_pending_copy 一致，最新件语义）
        self.selection_clear();
        self.select_notes_by_params(&pasted);
        self.mark_notes_changed();
        tracing::info!("Editor: 已粘贴 {} 个音符", pasted_count);
    }
}
