//! 工程走带剪贴板操作（复制/粘贴/剪切）
//!
//! 使用与钢琴卷帘相同的 JSON 剪贴板格式，额外包含 origin_track。

use super::Editor;
use super::helpers::ClipboardNoteEntry;
use super::helpers::note_event_to_note;
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
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(e) => {
                tracing::error!("Arrangement: 创建剪贴板失败: {}", e);
                return false;
            }
        };
        let text = match clipboard.get_text() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Arrangement: 读取剪贴板失败: {}", e);
                return false;
            }
        };
        self.arrange_paste_from_text(&text)
    }

    /// 从剪贴板 JSON 文本粘贴（绕过系统剪贴板，便于单元测试）。
    ///
    /// 粘贴锚点与音轨映射规则见 [`Self::parse_arrangement_clipboard_notes`]。
    pub(crate) fn arrange_paste_from_text(&mut self, text: &str) -> bool {
        let Some((origin_key, origin_track, notes_value)) = self.parse_clipboard_json_text(text)
        else {
            return false;
        };
        let Some((anchor_tick, _anchor_visual, pasted)) =
            self.parse_arrangement_clipboard_notes(origin_key, origin_track, &notes_value)
        else {
            return false;
        };

        if pasted.is_empty() {
            return false;
        }

        self.push_history();

        let (inserted_count, current_track_touched, affected_tracks) =
            self.apply_paste_internal(anchor_tick, origin_key, &pasted);

        if inserted_count == 0 {
            self.editor_state.data.discard_last_history();
            return false;
        }

        if current_track_touched {
            self.mark_notes_changed();
        }
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
        tracing::info!(
            "Arrangement: 已粘贴 {} 个音符 (anchor_tick={})",
            inserted_count, anchor_tick
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
        let editor_data = &self.editor_state.data;
        // 以视觉位置（侧边栏顺序）为基准，保留复制时的相对布局。
        // 粘贴时统一经 `document_track_at` 映射回文档音轨索引，
        // 避免 `track_visual_order` 非恒等（删轨/加轨/排序后）时错位。
        let origin_visual = all_notes
            .iter()
            .map(|(track, _)| editor_data.visual_position_of(*track).unwrap_or(*track))
            .min()
            .unwrap_or(0);

        let note_count = all_notes.len();
        let payload = serde_json::json!({
            "lumino": lumino_ui_core::constants::editor::CLIPBOARD_FORMAT,
            "version": lumino_ui_core::constants::editor::CLIPBOARD_VERSION,
            "type": "arrangement",
            "origin_tick": origin_tick,
            "origin_key": origin_key,
            "origin_track": origin_visual,
            "notes": all_notes.into_iter().map(|(track, note)| {
                let visual = editor_data.visual_position_of(track).unwrap_or(track);
                serde_json::json!({
                    "tick": note.tick - origin_tick,
                    "key": note.key - origin_key,
                    "length": note.length,
                    "velocity": note.velocity,
                    "channel": note.channel,
                    "track": visual - origin_visual,
                })
            }).collect::<Vec<_>>(),
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

    /// 从 MidiDocument 收集所有选中音符（track_notes 缓存已删除，统一读 document）。
    fn collect_selected_notes_for_clipboard(&self) -> Vec<(usize, Note)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let mut all_notes: Vec<(usize, Note)> = Vec::new();

        let Some(doc) = &editor_data.document else {
            return all_notes;
        };
        for track_idx in 0..doc.track_count() {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for note_event in editor_data.track_notes(track_idx) {
                if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key) {
                    let note = note_event_to_note(note_event);
                    all_notes.push((track_idx, note));
                }
            }
        }

        all_notes
    }

    /// 从剪贴板 JSON 文本解析走带视图专用的数据（与系统剪贴板解耦，便于测试）。
    fn parse_clipboard_json_text(&self, text: &str) -> Option<(u16, usize, Vec<serde_json::Value>)> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;

        let clipboard_type = value.get("type").and_then(|t| t.as_str());
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        // `origin_track` 现表示复制时的锚点视觉位置（见 `write_arrangement_clipboard`）。
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
    ///
    /// `pasted` 中的 `dest_track` 已由 [`Self::parse_arrangement_clipboard_notes`]
    /// 解析为文档音轨索引（视觉偏移经 `document_track_at` 转换），此处直接插入。
    /// 返回 (inserted_count, current_track_touched, affected_tracks)。
    fn apply_paste_internal(
        &mut self,
        anchor_tick: f32,
        origin_key: u16,
        pasted: &[ClipboardNoteEntry],
    ) -> (usize, bool, std::collections::HashSet<usize>) {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut inserted_count = 0usize;
        let mut affected_tracks: std::collections::HashSet<usize> =
            std::collections::HashSet::new();

        for (dest_doc, tick_offset, key_offset, length, velocity, channel) in pasted {
            let dest_track = *dest_doc;
            let note_tick = (anchor_tick + *tick_offset).max(0.0);
            let note_key = origin_key.saturating_add(*key_offset).min(127);
            let note = Note::from_raw(note_tick, note_key, *length, *velocity, *channel);

            // 2026-08 单一权威源：直接插入 document（按 start_tick 有序插入）
            let editor_data = &mut self.editor_state.data;
            if editor_data.insert_note(dest_track, note.clone()) {
                affected_tracks.insert(dest_track);
                if dest_track == current_track {
                    current_track_touched = true;
                }
                inserted_count += 1;
                // 2026-09 协作修复：粘贴（新增音符）需广播给对端，否则 B 端缺失。
                // note 已插入文档并分配真实 id，按位置反查取回后随事件发出。
                let id = self
                    .editor_state
                    .data
                    .note_id_at(dest_track, note.tick, note.key)
                    .unwrap_or(0);
                lumino_message::events::emit(lumino_message::events::Event::Window(
                    lumino_message::events::window::Event::local_note_added(
                        id,
                        note.tick,
                        note.key,
                        note.length,
                        note.velocity,
                        note.channel,
                        dest_track,
                    ),
                ));
            }
        }

        (inserted_count, current_track_touched, affected_tracks)
    }

    /// 计算粘贴锚点（视觉位置，即侧边栏顺序）。
    ///
    /// 优先使用选区最小视觉位置；选区为空时取当前音轨的视觉位置。
    /// 返回值为**视觉位置**，粘贴时由 [`Self::parse_arrangement_clipboard_notes`]
    /// 经 `document_track_at` 映射为文档音轨索引——这是修复「复制粘贴落到错误音轨」
    /// 的关键：选择矩形存的是视觉位置，不能当作文档索引直接使用。
    fn compute_anchor_visual(&self) -> usize {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        if selection.is_empty() {
            return editor_data
                .visual_position_of(editor_data.current_track)
                .unwrap_or(editor_data.current_track);
        }
        let mut min_visual = usize::MAX;
        for rect in &selection.rects {
            let v = rect.4 as usize; // 选择矩形存的是视觉位置（侧边栏顺序）
            if v < min_visual {
                min_visual = v;
            }
        }
        if min_visual == usize::MAX {
            editor_data
                .visual_position_of(editor_data.current_track)
                .unwrap_or(editor_data.current_track)
        } else {
            min_visual
        }
    }

    /// 从走带剪贴板 JSON 解析锚点坐标和音符列表。
    ///
    /// 粘贴位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - 音轨以选中区域的**最小视觉位置**为锚点，若选择为空则使用当前音轨的视觉位置
    /// - 剪贴板 `track` 字段是相对锚点的**视觉偏移**，粘贴时先加锚点视觉位置得到
    ///   目标视觉位置，再经 `document_track_at` 映射为文档音轨索引（见
    ///   [`Self::compute_anchor_visual`]）。全链路统一视觉空间，修复
    ///   `track_visual_order` 非恒等（删轨/加轨/排序后）时复制粘贴落到错误音轨。
    fn parse_arrangement_clipboard_notes(
        &self,
        _origin_key: u16,
        _origin_track: usize,
        notes_value: &[serde_json::Value],
    ) -> Option<(f32, usize, Vec<ClipboardNoteEntry>)> {
        let anchor_tick = self.snap_tick(self.playback_position);

        let anchor_visual = self.compute_anchor_visual();

        let editor_data = &self.editor_state.data;
        // 2026-08 单一权威源：音轨数从 document 统计（track_notes 缓存已删除）
        let max_visual = editor_data
            .document
            .as_ref()
            .map(|doc| doc.track_count())
            .unwrap_or(0)
            .max(1);
        let mut pasted: Vec<ClipboardNoteEntry> = Vec::with_capacity(notes_value.len());

        for item in notes_value {
            let tick_offset = item.get("tick")?.as_f64()? as f32;
            let key_offset = item.get("key")?.as_u64()? as u16;
            let length = item.get("length")?.as_f64()? as f32;
            let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
            let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
            // 剪贴板 `track` 是相对锚点的视觉偏移（非负）
            let track_offset = item.get("track").and_then(|t| t.as_i64()).unwrap_or(0);

            let dest_visual = (anchor_visual as i64 + track_offset).max(0) as usize;
            if dest_visual >= max_visual {
                continue;
            }
            // 视觉位置 → 文档音轨索引（track_visual_order 非恒等时精确映射）
            let dest_doc = editor_data.document_track_at(dest_visual);

            pasted.push((
                dest_doc,
                tick_offset,
                key_offset,
                length,
                velocity,
                channel,
            ));
        }

        Some((anchor_tick, anchor_visual, pasted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::{doc_with_notes, seed_notes};
    use crate::note::Note;

    /// 构造 3 轨 document，音符在 doc 轨 0 和 doc 轨 2，并设置排序后的视觉映射：
    /// 视觉 0 → doc 轨 2，视觉 1 → doc 轨 0，视觉 2 → doc 轨 1
    /// （模拟用户把轨 2 拖到顶部的排序场景 —— `track_visual_order` 非恒等）。
    fn editor_with_sorted_visual_order() -> Editor {
        let mut editor = Editor::default();
        let notes2 = vec![Note::from_raw(0.0, 64, 10.0, 100, 0)];
        editor.editor_state.data.document = Some(doc_with_notes(3, 2, &notes2));
        editor
            .editor_state
            .data
            .insert_note(0, Note::from_raw(0.0, 60, 10.0, 100, 0));
        editor.editor_state.data.track_visual_order = vec![2, 0, 1];
        editor
    }

    fn doc_track_note_count(editor: &Editor, track: usize) -> usize {
        editor.editor_state.data.track_notes(track).len()
    }

    /// 锚点计算必须把选择矩形的**视觉位置**映射回**文档音轨索引**。
    #[test]
    fn test_compute_anchor_visual_maps_to_document_track() {
        let mut editor = editor_with_sorted_visual_order();
        editor.editor_state.data.current_track = 0;

        // 无选区 → 当前音轨（doc 0）的视觉位置 = 1
        assert_eq!(editor.compute_anchor_visual(), 1);

        // 选区在视觉轨 0（对应 doc 轨 2）
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 0));
        assert_eq!(
            editor.compute_anchor_visual(),
            0,
            "选区视觉 0 应映射到视觉位置 0"
        );

        // 选区在视觉轨 1（对应 doc 轨 0）
        editor.editor_state.data.arrange_selection.rects.clear();
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 1, 1));
        assert_eq!(
            editor.compute_anchor_visual(),
            1,
            "选区视觉 1 应映射到视觉位置 1"
        );
    }

    /// 模拟 `write_arrangement_clipboard` 产出的 JSON：复制自 doc 轨 2（视觉 0），
    /// 锚点视觉位置 = 0，音符视觉偏移 = 0。
    fn single_track_clipboard_json() -> String {
        r#"{"type":"arrangement","origin_tick":0.0,"origin_key":64,"origin_track":0,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#.to_string()
    }

    /// 回归：复制粘贴（视觉偏移语义）在 `track_visual_order` 非恒等时落到正确文档音轨。
    #[test]
    fn test_paste_lands_on_mapped_document_track() {
        let mut editor = editor_with_sorted_visual_order();
        // 选区锚定在视觉轨 0（doc 轨 2）
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 0));

        let pasted = editor.arrange_paste_from_text(&single_track_clipboard_json());
        assert!(pasted, "粘贴应成功");
        // 视觉 0 → doc 轨 2，音符应落在 doc 轨 2，而非 doc 轨 0
        assert_eq!(doc_track_note_count(&editor, 2), 2, "doc 轨 2 应新增 1 个音符");
        assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应被误写入");
        assert_eq!(doc_track_note_count(&editor, 1), 0);
    }

    /// 回归：多轨复制粘贴保持视觉相对布局（核心修复点）。
    ///
    /// 复制 doc 轨 2（视觉 0）+ doc 轨 0（视觉 1），锚点视觉 0；
    /// 旧逻辑把剪贴板 `track` 当文档偏移，会落到 doc 0 / doc 1（错位）；
    /// 新逻辑按视觉偏移解析，应落回 doc 2 / doc 0。
    #[test]
    fn test_paste_multi_track_preserves_visual_layout() {
        let mut editor = editor_with_sorted_visual_order();
        // 选区覆盖视觉轨 0 与 1（doc 轨 2 与 doc 轨 0）
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 1));
        // 两音：doc 轨 2（视觉偏移 0）、doc 轨 0（视觉偏移 1）
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0},{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":1}]}"#;

        let pasted = editor.arrange_paste_from_text(json);
        assert!(pasted, "多轨粘贴应成功");
        // 视觉 0 → doc 2，视觉 1 → doc 0
        assert_eq!(doc_track_note_count(&editor, 2), 2, "doc 轨 2 应新增音符");
        assert_eq!(doc_track_note_count(&editor, 0), 2, "doc 轨 0 应新增音符");
        assert_eq!(doc_track_note_count(&editor, 1), 0, "doc 轨 1 不应被误写入");
    }

    /// 恒等映射（无排序）下粘贴行为与历史一致：视觉位置即文档索引。
    #[test]
    fn test_paste_identity_mapping_unchanged() {
        let mut editor = Editor::default();
        seed_notes(
            &mut editor,
            3,
            0,
            &[Note::from_raw(0.0, 60, 10.0, 100, 0)],
        );
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 1, 1));
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#;

        let pasted = editor.arrange_paste_from_text(json);
        assert!(pasted);
        // 视觉 1 = doc 1，应落在 doc 1
        assert_eq!(doc_track_note_count(&editor, 1), 1);
        assert_eq!(doc_track_note_count(&editor, 0), 1);
    }

    /// 回归：走带框选复制后切换音轨，粘贴应锚定到切换后的当前轨，
    /// 而非残留的旧框选音轨（"两个同时活跃的音轨"）。
    ///
    /// 复现路径：doc 轨 2（视觉 0）框选并复制 → 在走带卷帘区域点击 doc 轨 0
    /// （视觉 1）切轨 → 粘贴。`switch_to_track` 必须清空 `arrange_selection`，
    /// 否则粘贴会落到旧框选所在的 doc 2（"选中的音轨"），而非切换后的 doc 0。
    #[test]
    fn test_track_switch_clears_arrange_selection_so_paste_anchors_switched_track() {
        let mut editor = editor_with_sorted_visual_order();
        editor.editor_state.data.current_track = 2; // 复制时位于 doc 轨 2（视觉 0）
        // 用户在 doc 轨 2（视觉 0）框选并复制
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 100, 0, 127, 0, 0));

        // 在走带卷帘区域点击 doc 轨 0（视觉 1）切换当前轨
        editor.switch_to_track(0);

        // 复制自视觉 0（doc 2），origin_track=0，note 视觉偏移=0
        let json = single_track_clipboard_json();
        let pasted = editor.arrange_paste_from_text(&json);
        assert!(pasted, "粘贴应成功");

        assert_eq!(
            doc_track_note_count(&editor, 0),
            2,
            "粘贴应落到切换后的当前轨 doc 0（而非旧选区所在 doc 2）"
        );
        assert_eq!(
            doc_track_note_count(&editor, 2),
            1,
            "doc 2 不应被旧选区误写入"
        );
    }
}

