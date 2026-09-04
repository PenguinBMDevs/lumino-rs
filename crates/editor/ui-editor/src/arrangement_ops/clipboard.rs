//! 工程走带剪贴板操作（复制/粘贴/剪切，Lumino 程序本体间同步）
//!
//! 使用与钢琴卷帘相同的 JSON 剪贴板格式，额外包含 origin_track；
//! 载荷携带 `division`（源 PPQN），粘贴时若与目标文档 PPQN 不一致则按 ratio 重采样，
//! 保证跨 Lumino 进程粘贴出的音符长度与数据完全一致。

use super::Editor;
use super::helpers::ClipboardNoteEntry;
use super::helpers::note_event_to_note;
use crate::note::Note;
use lumino_midi_loader::NoteEvent;
use lumino_midi_model::clipboard::{
    ClipRecord, decode_clipboard_records, encode_clipboard, parse_clipboard_header,
};
use std::time::Instant;

/// 走带二进制剪贴板子格式哨兵（写入 `ClipRecord` 头的 `track_hint` 字段），
/// 与钢琴卷帘二进制（track_hint=0）区分，避免跨路径互相误读。
const ARRANGEMENT_BINARY_MARK: u16 = 0xFFFF;

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

    /// 从剪贴板粘贴音符到工程走带视图（Lumino 私有 JSON / 二进制，含 PPQN 一致性重采样）。
    ///
    /// 粘贴位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - 音轨以选中区域的最小音轨为锚点，若选择为空则使用当前音轨
    /// - KEY 保持与被复制音符相同（不改变 KEY 位置）
    ///
    /// Windows 下优先尝试紧凑二进制（`track_hint == ARRANGEMENT_BINARY_MARK`），
    /// 命中则毫秒级粘贴；否则退化为 JSON 文本路径（跨平台正确）。
    ///
    /// 返回是否有音符被粘贴。
    pub fn arrange_paste_notes_from_clipboard(&mut self) -> bool {
        // Windows：优先读取紧凑二进制（仅接受走带子格式哨兵，避免误读钢琴卷帘二进制）
        #[cfg(windows)]
        {
            if let Some(bytes) = crate::clipboard::sys::get_clipboard_binary()
                && self.arrange_paste_from_binary_bytes(&bytes)
            {
                return true;
            }
        }

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
    pub(crate) fn arrange_paste_from_text(&mut self, text: &str) -> bool {
        let Some((origin_key, origin_track, source_division, notes_value)) =
            self.parse_clipboard_json_text(text)
        else {
            return false;
        };
        let Some((anchor_tick, _anchor_visual, pasted)) = self.parse_arrangement_clipboard_notes(
            origin_key,
            origin_track,
            source_division,
            &notes_value,
        ) else {
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
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
        tracing::info!(
            "Arrangement: 已粘贴 {} 个音符 (anchor_tick={})",
            inserted_count,
            anchor_tick
        );
        true
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

    /// 走带二进制剪贴板编码（优化路径，对应钢琴卷帘 `build_clipboard_binary`）。
    ///
    /// 与 `write_arrangement_clipboard`（JSON）完全同语义：携带 origin 与视觉偏移、源 division，
    /// 但用紧凑二进制（`encode_clipboard`）替代 JSON 序列化——1M 音符从 ~2s 降到 ~25ms。
    /// `track_hint` 写入 `ARRANGEMENT_BINARY_MARK` 哨兵，粘贴端据此区分走带 / 钢琴卷帘二进制子格式。
    ///
    /// 视觉偏移（而非绝对音轨）编码进 `ClipRecord.track`：粘贴端 `dest_visual = anchor_visual
    /// + 偏移`，再经 `document_track_at` 映射回文档音轨，与 JSON 路径逐字节一致。
    fn encode_arrangement_clipboard_binary(
        &self,
        all_notes: &[(usize, NoteEvent)],
    ) -> Option<Vec<u8>> {
        if all_notes.is_empty() {
            return None;
        }
        let editor_data = &self.editor_state.data;
        let origin_tick = all_notes
            .iter()
            .map(|(_, n)| n.start_tick)
            .min()
            .unwrap_or(0);
        let origin_key = all_notes.iter().map(|(_, n)| n.key).min().unwrap_or(0);
        let origin_visual = all_notes
            .iter()
            .map(|(t, _)| editor_data.visual_position_of(*t).unwrap_or(*t))
            .min()
            .unwrap_or(0);
        let division = editor_data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let count = all_notes.len();
        let records: Vec<ClipRecord> = all_notes
            .iter()
            .map(|(track, n)| {
                let visual = editor_data.visual_position_of(*track).unwrap_or(*track);
                let track_offset = (visual as i64 - origin_visual as i64).max(0) as u16;
                ClipRecord::new(
                    n.start_tick - origin_tick,
                    n.end_tick - n.start_tick,
                    (n.key as i32 - origin_key as i32).max(0) as u8,
                    n.velocity,
                    n.channel,
                    track_offset,
                )
            })
            .collect();
        Some(encode_clipboard(
            records.into_iter(),
            count,
            division,
            origin_tick,
            origin_key,
            ARRANGEMENT_BINARY_MARK,
        ))
    }

    /// 从走带二进制剪贴板载荷粘贴（绕过系统剪贴板，便于测试与 Windows 二进制路径）。
    ///
    /// 与 `arrange_paste_from_text` 同语义：锚点对齐 playback_position、按视觉偏移落轨、
    /// 含 PPQN 一致性重采样；区别仅在载荷格式为紧凑二进制（毫秒级 vs JSON 秒级）。
    /// 仅当 `track_hint == ARRANGEMENT_BINARY_MARK` 时接受，避免误读钢琴卷帘二进制。
    pub(crate) fn arrange_paste_from_binary_bytes(&mut self, bytes: &[u8]) -> bool {
        puffin::profile_scope!("arrangement::paste_binary");
        let t0 = Instant::now();
        let meta = match parse_clipboard_header(bytes) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Arrangement: 二进制剪贴板头解析失败: {e}");
                return false;
            }
        };
        if meta.track_hint != ARRANGEMENT_BINARY_MARK {
            return false;
        }
        let editor_data = &self.editor_state.data;
        let target_div = editor_data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let ratio = if meta.division != 0 && meta.division != target_div {
            target_div as f64 / meta.division as f64
        } else {
            1.0
        };
        let anchor_tick = self.snap_tick(self.playback_position);
        let anchor_visual = self.compute_anchor_visual();
        let max_visual = editor_data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(0)
            .max(1);
        let origin_key = meta.origin_key as u16;
        let mut pasted: Vec<ClipboardNoteEntry> = Vec::with_capacity(meta.count as usize);
        if decode_clipboard_records(
            bytes,
            |tick_offset, length, key_offset, velocity, channel, track_offset_field| {
                let dest_visual =
                    (anchor_visual as i64 + track_offset_field as i64).max(0) as usize;
                if dest_visual >= max_visual {
                    return;
                }
                let dest_doc = editor_data.document_track_at(dest_visual);
                let (to, le) = if ratio == 1.0 {
                    (tick_offset as f32, length as f32)
                } else {
                    (
                        (tick_offset as f64 * ratio).round() as f32,
                        (length as f64 * ratio).round() as f32,
                    )
                };
                pasted.push((dest_doc, to, key_offset as u16, le, velocity, channel));
            },
        )
        .is_err()
        {
            return false;
        }
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
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            "Arrangement: 已粘贴 {} 个音符 (anchor_tick={}) [二进制] 耗时 {:.2}ms",
            inserted_count,
            anchor_tick,
            elapsed_ms
        );
        true
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
        // 去重：同一音符可能因重叠矩形被多次命中（用 id+位置做幂等键）
        let mut seen: std::collections::HashSet<(usize, u64, u32, u8)> =
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
                            note_event.id,
                            note_event.start_tick,
                            note_event.key,
                        ))
                    {
                        all_notes.push((doc_track, *note_event));
                    }
                }
            }
        }
        all_notes
    }

    /// 从剪贴板 JSON 文本解析走带视图专用的数据（与系统剪贴板解耦，便于测试）。
    fn parse_clipboard_json_text(
        &self,
        text: &str,
    ) -> Option<(u16, usize, Option<u16>, Vec<serde_json::Value>)> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;

        let clipboard_type = value.get("type").and_then(|t| t.as_str());
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        // `origin_track` 现表示复制时的锚点视觉位置（见 `write_arrangement_clipboard`）。
        let origin_track = value.get("origin_track")?.as_u64()? as usize;
        // 源 division（PPQN），用于粘贴端 PPQN 一致性重采样；缺失则视为与目标一致。
        let division = value
            .get("division")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);
        let notes = value.get("notes")?.as_array()?.to_vec();

        if clipboard_type == Some("arrangement") {
            Some((origin_key, origin_track, division, notes))
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
    ///
    /// P0 修复：按目标文档音轨分组批量插入（O(N·log M)），并直接拿回已分配 id 广播，
    /// 消除原逐条 `insert_note`（O(N·M) 插入）+ `note_id_at`（O(N·M) 广播）双重悬崖。
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

        // 按目标文档音轨分组，批量插入并直接取回已分配 id
        let mut by_track: std::collections::HashMap<usize, Vec<Note>> =
            std::collections::HashMap::new();
        for (dest_doc, tick_offset, key_offset, length, velocity, channel) in pasted {
            let note_tick = (anchor_tick + *tick_offset).max(0.0);
            let note_key = origin_key.saturating_add(*key_offset).min(127);
            let note = Note::from_raw(note_tick, note_key, *length, *velocity, *channel);
            by_track.entry(*dest_doc).or_default().push(note);
        }

        puffin::profile_scope!("arrangement::insert_notes");
        let t0 = Instant::now();
        let mut batch_acc: Vec<(u64, f32, u16, f32, u8, u8, usize)> = Vec::new();
        for (dest_track, notes) in by_track {
            let ids = self
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(dest_track, &notes);
            for (note, id) in notes.iter().zip(ids.iter()) {
                affected_tracks.insert(dest_track);
                if dest_track == current_track {
                    current_track_touched = true;
                }
                inserted_count += 1;
                batch_acc.push((
                    *id,
                    note.tick,
                    note.key,
                    note.length,
                    note.velocity,
                    note.channel,
                    dest_track,
                ));
            }
        }
        // 协作批量：走带粘贴同样改为批量消息
        if !batch_acc.is_empty() {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_notes_added_batch(batch_acc),
            ));
        }
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tracing::debug!(
            target: "perf::arrangement",
            inserted = inserted_count,
            ms = elapsed_ms,
            "insert_notes"
        );

        (inserted_count, current_track_touched, affected_tracks)
    }

    /// 计算粘贴锚点（视觉位置，即侧边栏顺序）。
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
            let v = rect.4 as usize;
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

    /// 从走带剪贴板 JSON 解析锚点坐标和音符列表（含 PPQN 一致性重采样）。
    ///
    /// 粘贴位置规则：X 对齐 playback_position；音轨以选中区域最小视觉位置为锚点；
    /// `track` 是相对锚点的视觉偏移。全链路统一视觉空间，修复 `track_visual_order`
    /// 非恒等（删轨/加轨/排序后）时复制粘贴落到错误音轨。
    ///
    /// PPQN 一致性：若 `source_division` 与当前文档 `division` 不一致且非零，则 tick
    /// 偏移与音符长度按 ratio = 目标/源 等比缩放（多一次同步计算），保证粘贴音符的
    /// 长度（节拍）与数据（key/vel/ch）与源完全一致。
    fn parse_arrangement_clipboard_notes(
        &self,
        _origin_key: u16,
        _origin_track: usize,
        source_division: Option<u16>,
        notes_value: &[serde_json::Value],
    ) -> Option<(f32, usize, Vec<ClipboardNoteEntry>)> {
        let anchor_tick = self.snap_tick(self.playback_position);

        let anchor_visual = self.compute_anchor_visual();

        let editor_data = &self.editor_state.data;
        let max_visual = editor_data
            .document
            .as_ref()
            .map(|doc| doc.track_count())
            .unwrap_or(0)
            .max(1);
        let target_division = editor_data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        // 多一次同步计算：PPQN 不一致时计算重采样 ratio
        let ratio = match source_division {
            Some(src) if src != 0 && src != target_division => target_division as f64 / src as f64,
            _ => 1.0,
        };

        let mut pasted: Vec<ClipboardNoteEntry> = Vec::with_capacity(notes_value.len());

        for item in notes_value {
            let raw_tick = item.get("tick")?.as_f64()? as f32;
            let raw_length = item.get("length")?.as_f64()? as f32;
            let key_offset = item.get("key")?.as_u64()? as u16;
            let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
            let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
            let track_offset = item.get("track").and_then(|t| t.as_i64()).unwrap_or(0);

            // tick 偏移与长度按 ratio 重采样（同 PPQN 时 ratio=1，零缩放、逐字节一致）
            let tick_offset = if ratio == 1.0 {
                raw_tick
            } else {
                (raw_tick as f64 * ratio).round() as f32
            };
            let length = if ratio == 1.0 {
                raw_length
            } else {
                (raw_length as f64 * ratio).round() as f32
            };

            let dest_visual = (anchor_visual as i64 + track_offset).max(0) as usize;
            if dest_visual >= max_visual {
                continue;
            }
            let dest_doc = editor_data.document_track_at(dest_visual);

            pasted.push((dest_doc, tick_offset, key_offset, length, velocity, channel));
        }

        Some((anchor_tick, anchor_visual, pasted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::tests::test_helpers::{doc_with_notes, seed_notes};

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

    #[test]
    fn test_compute_anchor_visual_maps_to_document_track() {
        let mut editor = editor_with_sorted_visual_order();
        editor.editor_state.data.current_track = 0;
        assert_eq!(editor.compute_anchor_visual(), 1);
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 0));
        assert_eq!(editor.compute_anchor_visual(), 0);
        editor.editor_state.data.arrange_selection.rects.clear();
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 1, 1));
        assert_eq!(editor.compute_anchor_visual(), 1);
    }

    fn single_track_clipboard_json() -> String {
        r#"{"type":"arrangement","origin_tick":0.0,"origin_key":64,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#.to_string()
    }

    #[test]
    fn test_paste_lands_on_mapped_document_track() {
        let mut editor = editor_with_sorted_visual_order();
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 0));
        let pasted = editor.arrange_paste_from_text(&single_track_clipboard_json());
        assert!(pasted, "粘贴应成功");
        assert_eq!(
            doc_track_note_count(&editor, 2),
            2,
            "doc 轨 2 应新增 1 个音符"
        );
        assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应被误写入");
        assert_eq!(doc_track_note_count(&editor, 1), 0);
    }

    #[test]
    fn test_paste_multi_track_preserves_visual_layout() {
        let mut editor = editor_with_sorted_visual_order();
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 0, 1));
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0},{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":1}]}"#;
        let pasted = editor.arrange_paste_from_text(json);
        assert!(pasted, "多轨粘贴应成功");
        assert_eq!(doc_track_note_count(&editor, 2), 2, "doc 轨 2 应新增音符");
        assert_eq!(doc_track_note_count(&editor, 0), 2, "doc 轨 0 应新增音符");
        assert_eq!(doc_track_note_count(&editor, 1), 0, "doc 轨 1 不应被误写入");
    }

    #[test]
    fn test_paste_identity_mapping_unchanged() {
        let mut editor = Editor::default();
        seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 1, 1));
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#;
        let pasted = editor.arrange_paste_from_text(json);
        assert!(pasted);
        assert_eq!(doc_track_note_count(&editor, 1), 1);
        assert_eq!(doc_track_note_count(&editor, 0), 1);
    }

    #[test]
    fn test_track_switch_clears_arrange_selection_so_paste_anchors_switched_track() {
        let mut editor = editor_with_sorted_visual_order();
        editor.editor_state.data.current_track = 2;
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 100, 0, 127, 0, 0));
        editor.switch_to_track(0);
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

    /// 回归：PPQN 不一致时粘贴需多一次重采样，使音符**长度（节拍）完全一致**。
    /// 源 division=480、length=480（=1 拍）；目标 division=960 → 应重采样为 960。
    #[test]
    fn test_paste_resamples_length_on_ppqn_mismatch() {
        let mut editor = Editor::default();
        editor.editor_state.data.document = Some(doc_with_notes(1, 0, &[]));
        // 目标文档 PPQN 设为 960（与源 480 不一致）
        editor
            .editor_state
            .data
            .document
            .as_mut()
            .expect("测试前已设置 document，应能取得可变借用")
            .division = 960;
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":480.0,"velocity":100,"channel":0,"track":0}]}"#;
        assert!(editor.arrange_paste_from_text(json));
        let notes = editor.editor_state.data.track_notes(0);
        assert_eq!(notes.len(), 1);
        // 1 拍在 480 PPQN 下 = 480 tick；在 960 PPQN 下应 = 960 tick（长度一致）
        assert_eq!(
            notes[0].length(),
            960u32,
            "PPQN 不一致时应重采样长度以保节拍一致"
        );
    }

    /// 回归：PPQN 一致（或缺失 division）时零缩放，粘贴音符长度逐字节一致。
    #[test]
    fn test_paste_no_resample_when_ppqn_matches() {
        let mut editor = Editor::default();
        editor.editor_state.data.document = Some(doc_with_notes(1, 0, &[]));
        // 默认 doc_with_notes division=480，与 JSON division=480 一致
        let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":480.0,"velocity":100,"channel":0,"track":0}]}"#;
        assert!(editor.arrange_paste_from_text(json));
        let notes = editor.editor_state.data.track_notes(0);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].length(), 480u32, "PPQN 一致时不应缩放");
    }

    /// 回归：二进制走带剪贴板往返与 JSON 路径**落轨一致**，且头哨兵正确。
    ///
    /// 与 `test_paste_identity_mapping_unchanged` 同构：粘贴目标带 selection 视觉区间 (1,1)
    /// （anchor_visual=1）、二进制音符 track 偏移=0，粘贴应落到 doc 1（而非 doc 0），
    /// 与 JSON 路径逐字节一致。
    #[test]
    fn test_arrangement_binary_roundtrip_matches_json() {
        let mut editor = Editor::default();
        seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
        // 单音符（doc 0，视觉 0 → track 偏移 0），division=480，origin_key=60，origin_tick=0
        let all_notes = vec![(0usize, NoteEvent::new(0, 10, 60, 100, 0))];
        let bytes = editor
            .encode_arrangement_clipboard_binary(&all_notes)
            .expect("二进制编码失败");
        let meta =
            lumino_midi_model::clipboard::parse_clipboard_header(&bytes).expect("头解析失败");
        assert_eq!(
            meta.track_hint, ARRANGEMENT_BINARY_MARK,
            "二进制头应写入走带子格式哨兵"
        );

        // 粘贴目标带 selection 视觉区间 (1,1) → anchor_visual=1，与 JSON 测试一致
        editor
            .editor_state
            .data
            .arrange_selection
            .rects
            .push((0, 10, 0, 127, 1, 1));
        assert!(
            editor.arrange_paste_from_binary_bytes(&bytes),
            "二进制粘贴应成功"
        );
        assert_eq!(
            doc_track_note_count(&editor, 1),
            1,
            "二进制粘贴落轨应与 JSON 路径一致（doc 1）"
        );
        assert_eq!(
            doc_track_note_count(&editor, 0),
            1,
            "原始种子音符仍应在 doc 0"
        );
    }
}
