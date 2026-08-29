//! 剪贴板操作：复制、剪切、粘贴音符（Lumino 程序本体间同步）
//!
//! 跨 Lumino 程序实例（多进程）的音符同步：复制时把选区写成 Lumino 私有 JSON 文本
//! （经 `arboard` 入系统剪贴板），粘贴时读回并还原。两端文档 PPQN 可能不同，因此
//! 复制载荷里携带**源 division**，粘贴时若与目标 division 不一致，就**多算一次重采样**
//! （ratio = 目标 PPQN / 源 PPQN）把 tick 偏移与音符长度等比缩放，保证粘贴出的音符
//! 与源音符「长度与数据完全一致」（同 PPQN 时零缩放、逐字节一致）。

use super::Editor;
use lumino_ui_core::constants::editor::{CLIPBOARD_FORMAT, CLIPBOARD_VERSION};

#[cfg(windows)]
mod sys;

#[cfg(windows)]
use lumino_midi_model::clipboard::{
    decode_clipboard_chunks, encode_clipboard, parse_clipboard_header, ClipRecord,
};

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

    /// 将选中音符复制到系统剪贴板。
    ///
    /// 优先级：Windows 下优先写入**紧凑二进制私有格式**（`LuminoMidiNotes`），
    /// 内存/速度远优于文本 JSON，且跨 Lumino 实例零拷贝；若二进制不可用，则退化为
    /// Lumino 私有 JSON 文本（跨平台正确）。两者都携带 `division`（源 PPQN），
    /// 供粘贴端做 PPQN 一致性重采样，保证「长度与数据完全一致」。
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

        // Windows：优先二进制私有格式
        #[cfg(windows)]
        {
            if let Some(bytes) = self.build_clipboard_binary(track, division)
                && crate::clipboard::sys::set_clipboard_binary(&bytes)
            {
                tracing::info!(
                    "Editor: 已复制 {} 字节二进制音符 (division={})",
                    bytes.len(),
                    division
                );
                return true;
            }
        }

        // 退化：非 Windows 或二进制不可用 → 文本 JSON
        match self.build_clipboard_json(track, division) {
            Some(s) => {
                let ok = set_clipboard_text(&s);
                if ok {
                    tracing::info!("Editor: 已复制音符 (文本 JSON, division={})", division);
                }
                ok
            }
            None => false,
        }
    }

    /// 构建 Lumino 私有 JSON 剪贴板文本（跨平台退化路径）。
    ///
    /// 两遍扫描选中音符：第一遍算 origin，第二遍流式拼 JSON，不物化 `Vec<Value>`。
    fn build_clipboard_json(&self, track: usize, division: u16) -> Option<String> {
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
        });
        if min_tick.is_infinite() {
            return None;
        }
        let origin_tick = if min_tick.is_finite() { min_tick } else { 0.0 };
        let origin_key = if min_key != u16::MAX { min_key } else { 0 };

        let mut s = String::with_capacity(2048);
        use std::fmt::Write as _;
        let _ = write!(
            s,
            "{{\"lumino\":\"{}\",\"version\":{},\"track\":{},\"origin_tick\":{},\"origin_key\":{},\"division\":{},\"notes\":[",
            CLIPBOARD_FORMAT, CLIPBOARD_VERSION, track, origin_tick, origin_key, division
        );
        let mut first = true;
        self.each_selected_note_on_current_track(|n| {
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
        });
        s.push_str("]}");
        Some(s)
    }

    /// 构建紧凑二进制剪贴板载荷（Windows 二进制路径）。
    ///
    /// **流式、零大数组**：第一遍扫描选中音符算 origin（min tick/key），第二遍按文档
    /// tick 顺序 `filter_map` 出 `ClipRecord` 直接喂给 `encode_clipboard`，不物化任何
    /// `Vec<NoteEvent>` / `Vec<ClipRecord>`，故「全选」10M 音符也只占用约 67MB 载荷内存。
    #[cfg(windows)]
    fn build_clipboard_binary(&self, track: usize, division: u16) -> Option<Vec<u8>> {
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.current_track_notes();

        // 第一遍：origin
        let mut min_tick = u32::MAX;
        let mut min_key = u8::MAX;
        let mut count = 0usize;
        let mut visit = |n: &lumino_midi_loader::NoteEvent| {
            if n.start_tick < min_tick {
                min_tick = n.start_tick;
            }
            if n.key < min_key {
                min_key = n.key;
            }
            count += 1;
        };
        if let Some(bs) = &interaction.selection_bitset {
            for (i, n) in notes.iter().enumerate() {
                if bs.get(i) {
                    visit(n);
                }
            }
        } else {
            for &i in &interaction.selected_notes {
                if let Some(n) = notes.get(i) {
                    visit(n);
                }
            }
        }
        if count == 0 {
            return None;
        }
        let origin_tick = if min_tick != u32::MAX { min_tick } else { 0 };
        let origin_key = if min_key != u8::MAX { min_key } else { 0 };

        // 第二遍：流式编码（文档顺序即 tick 升序；delta 编码使密集排布极省）
        let bytes = encode_clipboard(
            notes.iter().enumerate().filter_map(|(i, n)| {
                let sel = if let Some(bs) = &interaction.selection_bitset {
                    bs.get(i)
                } else {
                    interaction.selected_notes.contains(&i)
                };
                if !sel {
                    return None;
                }
                Some(ClipRecord::new(
                    n.start_tick - origin_tick,
                    n.end_tick - n.start_tick,
                    (n.key as i32 - origin_key as i32).max(0) as u8,
                    n.velocity,
                    n.channel,
                    track as u16,
                ))
            }),
            division,
            origin_tick,
            origin_key,
            track as u16,
        );
        Some(bytes)
    }

    /// 从剪贴板粘贴音符。
    ///
    /// Windows：先探测 Lumino 二进制私有格式，命中则走紧凑二进制粘贴（含 PPQN 重采样）；
    /// 否则退化为 Lumino 私有 JSON 文本粘贴（跨平台正确）。
    pub(crate) fn paste_notes_from_clipboard(&mut self) {
        #[cfg(windows)]
        {
            if self.try_paste_from_binary() {
                return;
            }
        }
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

    /// 二进制私有格式粘贴（Windows）。
    ///
    /// 分块解码 → 锚点定位 → 含 PPQN 重采样的批量插入。`decode_clipboard_chunks`
    /// 逐块回调，每块仅物化一个 `Vec<Note>`，故 10M 音符粘贴也只占用约 67MB 载荷 +
    /// 单块的工作内存，不会瞬时分配 GB 级数组。
    #[cfg(windows)]
    fn try_paste_from_binary(&mut self) -> bool {
        let bytes = match crate::clipboard::sys::get_clipboard_binary() {
            Some(b) => b,
            None => return false,
        };
        let meta = match parse_clipboard_header(&bytes) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let target_div = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        // PPQN 一致性：源/目标不一致才重采样（ratio=1 时逐字节一致）
        let ratio = if meta.division != 0 && meta.division != target_div {
            target_div as f64 / meta.division as f64
        } else {
            1.0
        };
        let anchor_tick = self.snap_tick(self.playback_position);
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let track = self.editor_state.data.current_track;

        self.push_history();
        self.selection_clear();

        let mut total = 0usize;
        let res = decode_clipboard_chunks(&bytes, 100_000, |recs| {
            let notes: Vec<super::Note> = recs
                .iter()
                .map(|r| {
                    let tick_offset = if ratio == 1.0 {
                        r.tick_offset as f64
                    } else {
                        (r.tick_offset as f64 * ratio).round()
                    };
                    let length = if ratio == 1.0 {
                        r.length as f64
                    } else {
                        (r.length as f64 * ratio).round()
                    };
                    let tick = (anchor_tick + tick_offset as f32).max(0.0);
                    let key = (meta.origin_key as i32 + r.key_offset as i32)
                        .max(0)
                        .min(max_key as i32) as u16;
                    super::Note::from_raw(
                        tick,
                        key,
                        length as f32,
                        r.velocity,
                        r.channel,
                    )
                })
                .collect();
            let ids = self
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(track, &notes);
            for (n, id) in notes.iter().zip(ids) {
                lumino_message::events::emit(lumino_message::events::Event::Window(
                    lumino_message::events::window::Event::local_note_added(
                        id, n.tick, n.key, n.length, n.velocity, n.channel, track,
                    ),
                ));
            }
            total += notes.len();
        });

        if res.is_err() || total == 0 {
            return false;
        }
        self.mark_notes_changed();
        tracing::info!("Editor: 已从二进制剪贴板粘贴 {} 个音符", total);
        true
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
