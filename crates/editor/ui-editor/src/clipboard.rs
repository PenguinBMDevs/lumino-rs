//! 剪贴板操作：复制、剪切、粘贴音符（Lumino 程序本体间同步）
//!
//! 跨 Lumino 程序实例（多进程）的音符同步：复制时把选区写成 Lumino 私有 JSON 文本
//! （经 `arboard` 入系统剪贴板），粘贴时读回并还原。两端文档 PPQN 可能不同，因此
//! 复制载荷里携带**源 division**，粘贴时若与目标 division 不一致，就**多算一次重采样**
//! （ratio = 目标 PPQN / 源 PPQN）把 tick 偏移与音符长度等比缩放，保证粘贴出的音符
//! 与源音符「长度与数据完全一致」（同 PPQN 时零缩放、逐字节一致）。

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

    /// 将选中音符复制到系统剪贴板（Lumino 私有 JSON）。
    ///
    /// 性能：流式写出 JSON，避免物化 `Vec<Value>`（百万级即 GB 级分配）；
    /// 选区遍历不收集索引 Vec，避免「全选」时 2.3GB 分配。
    /// 载荷携带 `division`（源 PPQN），供粘贴端做 PPQN 一致性重采样。
    pub(crate) fn copy_selected_notes_to_clipboard(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let track = self.editor_state.data.current_track;
        let division = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);

        // 收集选中音符（NoteEvent，u32 tick 保精度）并算 origin
        let mut selected: Vec<lumino_midi_loader::NoteEvent> = Vec::new();
        let mut min_tick = f32::INFINITY;
        let mut min_key = u16::MAX;
        self.each_selected_note_on_current_track(|n| {
            let t = n.start_tick as f32;
            if t < min_tick {
                min_tick = t;
            }
            let k = n.key as u16;
            if k < min_key {
                min_key = k;
            }
            selected.push(*n);
        });
        let count = selected.len();
        if count == 0 {
            return false;
        }
        let origin_tick = if min_tick.is_finite() { min_tick } else { 0.0 };
        let origin_key = if min_key != u16::MAX { min_key } else { 0 };

        // 流式写出 JSON 音符片段（避免 Vec<Value> 的 GB 级分配）
        let mut s = String::with_capacity(count.saturating_mul(40) + 160);
        use std::fmt::Write as _;
        let _ = write!(
            s,
            "{{\"lumino\":\"{}\",\"version\":{},\"track\":{},\"origin_tick\":{},\"origin_key\":{},\"division\":{},\"notes\":[",
            CLIPBOARD_FORMAT, CLIPBOARD_VERSION, track, origin_tick, origin_key, division
        );
        let mut first = true;
        for n in &selected {
            let tick = (n.start_tick as f32 - origin_tick).max(0.0);
            let key = (n.key as i32 - origin_key as i32).max(0) as u16;
            let length = (n.end_tick - n.start_tick) as f32;
            if !first {
                s.push(',');
            }
            first = false;
            let _ = write!(
                s,
                "{{\"tick\":{},\"key\":{},\"length\":{},\"velocity\":{},\"channel\":{}}}",
                tick, key, length, n.velocity, n.channel
            );
        }
        s.push_str("]}");

        let ok = set_clipboard_text(&s);
        if ok {
            tracing::info!("Editor: 已复制 {} 个音符 (division={})", count, division);
        }
        ok
    }

    /// 从剪贴板粘贴音符（Lumino 私有 JSON，含 PPQN 一致性重采样）
    pub(crate) fn paste_notes_from_clipboard(&mut self) {
        let Some((origin_key, source_division, notes_value)) = self.read_clipboard_json() else {
            return;
        };
        if let Some((anchor, pasted)) =
            self.parse_clipboard_notes(origin_key, source_division, &notes_value)
            && !pasted.is_empty()
        {
            self.commit_pasted_notes(anchor, pasted);
        }
    }

    /// 从剪贴板读取并解析 JSON 数据，返回 (origin_key, 源 division, notes 数组)
    fn read_clipboard_json(&self) -> Option<(u16, Option<u16>, Vec<serde_json::Value>)> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let text = clipboard.get_text().ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        let division = value
            .get("division")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);
        let notes = value.get("notes")?.as_array()?.to_vec();
        Some((origin_key, division, notes))
    }

    /// 从剪贴板 JSON 解析锚点坐标和音符列表，并按 PPQN 一致性重采样。
    ///
    /// 复制位置规则：
    /// - X 坐标（tick）对齐演奏指示线（playback_position）
    /// - Y 坐标（key）保持与被复制音符相同（origin_key）
    ///
    /// PPQN 一致性：若 `source_division` 与当前文档 `division` 不一致且非零，
    /// 则 tick 偏移与音符长度按 ratio = 目标/源 等比缩放（多一次同步计算），
    /// 使粘贴音符的**长度（节拍）与数据（key/vel/ch）与源完全一致**。
    fn parse_clipboard_notes(
        &self,
        origin_key: u16,
        source_division: Option<u16>,
        notes_value: &[serde_json::Value],
    ) -> Option<((f32, u16), Vec<super::Note>)> {
        let target_division = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let ratio = match source_division {
            Some(src) if src != 0 && src != target_division => {
                target_division as f64 / src as f64
            }
            _ => 1.0,
        };

        let anchor = (self.snap_tick(self.playback_position), origin_key);
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        let pasted: Vec<super::Note> = notes_value
            .iter()
            .filter_map(|item| {
                let raw_tick = item.get("tick")?.as_f64()? as f32;
                let raw_length = item.get("length")?.as_f64()? as f32;
                let key_offset = item.get("key")?.as_u64()? as u16;
                let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
                // 多一次同步计算：PPQN 不一致时重采样 tick 偏移与长度
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
                let tick = (anchor.0 + tick_offset).max(0.0);
                let key = anchor.1.saturating_add(key_offset).min(max_key);
                Some(super::Note::from_raw(
                    tick, key, length, velocity, channel,
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
        let track = self.editor_state.data.current_track;
        // P0 修复：批量插入直接回传已分配的全局唯一 id，O(N) 完成协作广播，
        // 消除原 `note_id_at` 对每条粘贴音符做全轨线性重扫的 O(N·M) 悬崖。
        let ids = self.editor_state.data.batch_insert_notes_with_ids(&pasted);
        for (n, id) in pasted.iter().zip(ids) {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_added(
                    id, n.tick, n.key, n.length, n.velocity, n.channel, track,
                ),
            ));
        }
        // 批量插入索引散布，旧连续选中在 tick 重叠时失效 → 按参数全等重选（最新件语义）
        self.selection_clear();
        self.select_notes_by_params(&pasted);
        self.mark_notes_changed();
        tracing::info!("Editor: 已粘贴 {} 个音符", pasted_count);
    }

    /// 遍历当前轨被选中音符（不物化索引 Vec，避免全选时 GB 级分配）
    fn each_selected_note_on_current_track(&self, mut f: impl FnMut(&lumino_midi_loader::NoteEvent)) {
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.current_track_notes();
        if let Some(ref bs) = interaction.selection_bitset {
            bs.for_each_set(|i| {
                if let Some(n) = notes.get(i) {
                    f(n);
                }
            });
        } else {
            for &i in &interaction.selected_notes {
                if let Some(n) = notes.get(i) {
                    f(n);
                }
            }
        }
    }
}

/// 仅写文本到系统剪贴板（arboard，跨平台）
pub(crate) fn set_clipboard_text(text: &str) -> bool {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            tracing::error!("Editor: 创建剪贴板失败: {}", e);
            return false;
        }
    };
    match clipboard.set_text(text.to_string()) {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("Editor: 复制到剪贴板失败: {}", e);
            false
        }
    }
}
