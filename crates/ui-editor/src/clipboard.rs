//! 剪贴板操作：复制、剪切、粘贴音符

use super::Editor;
use iced_core::Point;
use lumino_ui_constants::editor::{CLIPBOARD_FORMAT, CLIPBOARD_VERSION, DEFAULT_PASTE_ANCHOR_KEY};

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

        let notes: Vec<&super::Note> = indices
            .into_iter()
            .filter_map(|index| self.editor_state.data.notes.get(index))
            .collect();

        if notes.is_empty() {
            return false;
        }

        let origin_tick = notes
            .iter()
            .map(|note| note.tick)
            .fold(f32::INFINITY, f32::min);
        let origin_key = notes.iter().map(|note| note.key).min().unwrap_or(0);

        let payload = serde_json::json!({
            "lumino": CLIPBOARD_FORMAT,
            "version": CLIPBOARD_VERSION,
            "track": self.editor_state.data.current_track,
            "origin_tick": origin_tick,
            "origin_key": origin_key,
            "notes": notes.into_iter().map(|note| serde_json::json!({
                "tick": note.tick - origin_tick,
                "key": note.key - origin_key,
                "length": note.length,
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
        let Some(notes_value) = self.read_clipboard_json() else {
            return;
        };

        let Some((anchor, pasted)) = self.parse_clipboard_notes(&notes_value) else {
            return;
        };

        if pasted.is_empty() {
            return;
        }

        self.commit_pasted_notes(anchor, pasted);
    }

    /// 从剪贴板读取并解析 JSON 数据，返回 notes 数组
    fn read_clipboard_json(&self) -> Option<Vec<serde_json::Value>> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let text = clipboard.get_text().ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let notes = value.get("notes")?.as_array()?.to_vec();
        Some(notes)
    }

    /// 从剪贴板 JSON 解析锚点坐标和音符列表
    fn parse_clipboard_notes(
        &self,
        notes_value: &[serde_json::Value],
    ) -> Option<((f32, u16), Vec<super::Note>)> {
        let anchor = self
            .editor_state
            .canvas
            .cursor_position
            .filter(|pos| self.is_inside_canvas(Point::new(pos.0, pos.1)))
            .map(|pos| (self.snap_tick(self.x_to_tick(pos.0)), self.y_to_key(pos.1)))
            .unwrap_or((self.playback_position, DEFAULT_PASTE_ANCHOR_KEY));

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let pasted: Vec<super::Note> = notes_value
            .iter()
            .filter_map(|item| {
                let tick_offset = item.get("tick")?.as_f64()?;
                let key_offset = item.get("key")?.as_u64()? as u16;
                let length = item.get("length")?.as_f64()?;
                let tick = (anchor.0 + tick_offset as f32).max(0.0);
                let key = anchor.1.saturating_add(key_offset).min(max_key);
                Some(super::Note::new(tick, key, length as f32))
            })
            .collect();

        Some((anchor, pasted))
    }

    /// 将解析的音符提交到编辑器并选中
    fn commit_pasted_notes(&mut self, _anchor: (f32, u16), pasted: Vec<super::Note>) {
        self.push_history();
        self.selection_clear();
        let pasted_count = pasted.len();
        let start = self.editor_state.data.notes.len();
        self.editor_state.data.notes.extend(pasted);
        self.editor_state.data.track_notes.insert(
            self.editor_state.data.current_track,
            self.editor_state.data.notes.clone(),
        );
        self.editor_state.data.mark_track_notes_changed();
        for index in start..start + pasted_count {
            self.selection_insert(index);
        }
        self.mark_notes_changed();
        tracing::info!("Editor: 已粘贴 {} 个音符", pasted_count);
    }
}
