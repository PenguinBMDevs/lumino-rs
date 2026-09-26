//! 走带剪贴板复制/剪切：选区收集、JSON 写入与剪切编排（原 clipboard.rs 主体）

use super::super::helpers::note_event_to_note;
use crate::Editor;
use lumino_midi_loader::NoteEvent;

impl Editor {
    /// 复制工程走带选中音符到系统剪贴板（JSON，含 division）。
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

        // Windows：优先写入紧凑二进制（Lumino 私有格式），速度远优于 JSON，跨实例零拷贝；
        // 二进制不可用则退化为 JSON 文本（跨平台正确）。
        #[cfg(windows)]
        {
            if let Some(bytes) = self.encode_arrangement_clipboard_binary(&all_notes)
                && crate::clipboard::sys::set_clipboard_binary(&bytes)
            {
                tracing::info!(
                    "Arrangement: 已复制 {} 字节二进制音符 (division={})",
                    bytes.len(),
                    self.editor_state
                        .data
                        .document
                        .as_ref()
                        .map(|d| d.division)
                        .unwrap_or(480)
                );
                return true;
            }
        }

        self.write_arrangement_clipboard(all_notes)
    }

    /// 剪切工程走带选中音符（复制 + 删除）。
    pub fn arrange_cut_selected_notes(&mut self) -> usize {
        let copied = self.arrange_copy_selected_notes();
        if !copied {
            return 0;
        }
        self.arrange_delete_selected_notes()
    }

    // ── 私有辅助方法 ─────────────────────────────────────

    /// 构建并写入剪贴板 JSON（携带 division）。
    fn write_arrangement_clipboard(&self, all_notes: Vec<(usize, NoteEvent)>) -> bool {
        let editor_data = &self.editor_state.data;
        let division = editor_data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let origin_tick = all_notes
            .iter()
            .map(|(_, note)| note.start_tick as f32)
            .fold(f32::INFINITY, f32::min);
        let origin_key = all_notes
            .iter()
            .map(|(_, note)| note.key as u16)
            .min()
            .unwrap_or(0);
        let origin_visual = all_notes
            .iter()
            .map(|(track, _)| editor_data.visual_position_of(*track).unwrap_or(*track))
            .min()
            .unwrap_or(0);

        let note_count = all_notes.len();
        let mut s = String::with_capacity(note_count.saturating_mul(48) + 180);
        use std::fmt::Write as _;
        let _ = write!(
            s,
            "{{\"lumino\":\"{}\",\"version\":{},\"type\":\"arrangement\",\"origin_tick\":{},\"origin_key\":{},\"origin_track\":{},\"division\":{},\"notes\":[",
            lumino_ui_core::constants::editor::CLIPBOARD_FORMAT,
            lumino_ui_core::constants::editor::CLIPBOARD_VERSION,
            origin_tick,
            origin_key,
            origin_visual,
            division
        );
        let mut first = true;
        for (track, note_event) in &all_notes {
            let n = note_event_to_note(note_event);
            let visual = editor_data.visual_position_of(*track).unwrap_or(*track);
            let tick = (n.tick - origin_tick).max(0.0);
            let key = (n.key as i32 - origin_key as i32).max(0) as u16;
            let length = n.length;
            let track_offset = (visual as i64 - origin_visual as i64).max(0) as usize;
            if !first {
                s.push(',');
            }
            first = false;
            let _ = write!(
                s,
                "{{\"tick\":{},\"key\":{},\"length\":{},\"velocity\":{},\"channel\":{},\"track\":{}}}",
                tick, key, length, n.velocity, n.channel, track_offset
            );
        }
        s.push_str("]}");

        crate::clipboard::set_clipboard_text(&s)
    }

    /// 从 MidiDocument 收集所有选中音符（NoteEvent，u32 tick 保精度）。
    ///
    /// P1 修复：按选区矩形窗口反查命中音符，复杂度 O(rects × 窗口)；
    /// 替代原「遍历全曲所有音符 + selection.contains」的 O(全音符) 全扫。
    fn collect_selected_notes_for_clipboard(&self) -> Vec<(usize, NoteEvent)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let mut all_notes: Vec<(usize, NoteEvent)> = Vec::new();
        if editor_data.document.is_none() {
            return all_notes;
        }
        // 去重：同一音符可能因重叠矩形被多次命中（用值做幂等键，无 ID）
        let mut seen: std::collections::HashSet<(usize, u32, u32, u8, u8, u8)> =
            std::collections::HashSet::new();
        for &(ts, te, kl, kh, tl, th) in &selection.rects {
            for v in tl..=th {
                let doc_track = editor_data.document_track_at(v as usize);
                let notes = editor_data.track_notes(doc_track);
                let (lo, hi) = notes.window_range(ts, te, 0);
                for (_, note_event) in notes.iter_window(lo, hi) {
                    if note_event.key >= kl
                        && note_event.key <= kh
                        && note_event.start_tick >= ts
                        && note_event.start_tick < te
                        && seen.insert((
                            doc_track,
                            note_event.start_tick,
                            note_event.end_tick,
                            note_event.key,
                            note_event.velocity,
                            note_event.channel,
                        ))
                    {
                        all_notes.push((doc_track, *note_event));
                    }
                }
            }
        }
        all_notes
    }
}
