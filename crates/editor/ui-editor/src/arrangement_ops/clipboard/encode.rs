//! 走带二进制剪贴板编码（Lumino 私有紧凑格式：哨兵 + 视觉偏移）

use super::ARRANGEMENT_BINARY_MARK;
use crate::Editor;
use lumino_midi_loader::NoteEvent;
use lumino_midi_model::clipboard::{ClipRecord, encode_clipboard};

impl Editor {
    /// 走带二进制剪贴板编码（优化路径，对应钢琴卷帘 `build_clipboard_binary`）。
    ///
    /// 与 `write_arrangement_clipboard`（JSON）完全同语义：携带 origin 与视觉偏移、源 division，
    /// 但用紧凑二进制（`encode_clipboard`）替代 JSON 序列化——1M 音符从 ~2s 降到 ~25ms。
    /// `track_hint` 写入 `ARRANGEMENT_BINARY_MARK` 哨兵，粘贴端据此区分走带 / 钢琴卷帘二进制子格式。
    ///
    /// 视觉偏移（而非绝对音轨）编码进 `ClipRecord.track`：粘贴端 `dest_visual = anchor_visual
    /// + 偏移`，再经 `document_track_at` 映射回文档音轨，与 JSON 路径逐字节一致。
    ///
    /// 非 Windows 构建时仅单测调用（正式调用点为 `#[cfg(windows)]`），允许死代码以过 `-D warnings`。
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn encode_arrangement_clipboard_binary(
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
}
