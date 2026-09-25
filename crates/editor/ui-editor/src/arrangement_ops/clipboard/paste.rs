//! 走带剪贴板粘贴：文本/二进制载荷解析、锚点计算与批量落轨

use super::super::helpers::ClipboardNoteEntry;
use super::ARRANGEMENT_BINARY_MARK;
use crate::Editor;
use crate::note::Note;
use lumino_midi_model::clipboard::{decode_clipboard_records, parse_clipboard_header};
use std::time::Instant;

impl Editor {
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

    /// 从走带二进制剪贴板载荷粘贴（绕过系统剪贴板，便于测试与 Windows 二进制路径）。
    ///
    /// 与 `arrange_paste_from_text` 同语义：锚点对齐 playback_position、按视觉偏移落轨、
    /// 含 PPQN 一致性重采样；区别仅在载荷格式为紧凑二进制（毫秒级 vs JSON 秒级）。
    /// 仅当 `track_hint == ARRANGEMENT_BINARY_MARK` 时接受，避免误读钢琴卷帘二进制。
    ///
    /// 非 Windows 构建时仅单测调用（正式调用点为 `#[cfg(windows)]`），允许死代码以过 `-D warnings`。
    #[cfg_attr(not(windows), allow(dead_code))]
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
        let collab_sync = self.editor_state.data.collab_sync_enabled();
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
                // 协作同步关闭时不构建批量广播载荷。
                if collab_sync {
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
        }
        // 协作批量：走带粘贴同样改为批量消息（协作同步关闭时不发射）。
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
    pub(super) fn compute_anchor_visual(&self) -> usize {
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
