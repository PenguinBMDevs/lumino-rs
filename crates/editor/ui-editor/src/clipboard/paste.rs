use super::*;

impl Editor {
    /// 二进制私有格式粘贴（Windows）。
    ///
    /// 分块解码 → 锚点定位 → 含 PPQN 重采样的批量插入。`decode_clipboard_chunks`
    /// 逐块回调，每块仅物化一个 `Vec<Note>`，故 10M 音符粘贴也只占用约 67MB 载荷 +
    /// 单块的工作内存，不会瞬时分配 GB 级数组。
    #[cfg(windows)]
    pub(super) fn try_paste_from_binary(&mut self) -> bool {
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
                    super::Note::from_raw(tick, key, length as f32, r.velocity, r.channel)
                })
                .collect();
            let ids = self
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(track, &notes);
            if !ids.is_empty() {
                let batch: Vec<(u64, f32, u16, f32, u8, u8, usize)> = notes
                    .iter()
                    .zip(ids.iter())
                    .map(|(n, id)| (*id, n.tick, n.key, n.length, n.velocity, n.channel, track))
                    .collect();
                lumino_message::events::emit(lumino_message::events::Event::Window(
                    lumino_message::events::window::Event::local_notes_added_batch(batch),
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

    /// Domino（TAKABO SOFT）剪贴板粘贴（Windows）。
    ///
    /// 读取 `MidiPortalSequence` 格式 → `decode_domino_clipboard` 还原为 `NoteEvent` →
    /// 按目标文档 PPQN 做一致性重采样（样本采用 division=480）→ 插入当前轨并广播协作事件。
    #[cfg(windows)]
    pub(super) fn try_paste_from_domino(&mut self) -> bool {
        let raw = match crate::clipboard::sys::get_clipboard_domino() {
            Some(b) => b,
            None => return false,
        };
        let events = match decode_domino_clipboard(&raw) {
            Ok(n) => n,
            Err(e) => {
                tracing::debug!("Editor: Domino 剪贴板解码失败: {e}");
                return false;
            }
        };
        if events.is_empty() {
            return false;
        }
        // Domino 样本以 division=480 表达 tick；与目标不一致时等比重采样
        let target_div = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let ratio = if target_div != 480 {
            target_div as f64 / 480.0
        } else {
            1.0
        };
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let pasted: Vec<super::Note> = events
            .iter()
            .map(|n| {
                let tick = if ratio == 1.0 {
                    n.start_tick as f64
                } else {
                    (n.start_tick as f64 * ratio).round()
                };
                let length = if ratio == 1.0 {
                    n.length() as f64
                } else {
                    (n.length() as f64 * ratio).round()
                };
                let key = (n.key as i32).max(0).min(max_key as i32) as u16;
                super::Note::from_raw(tick as f32, key, length as f32, n.velocity, n.channel)
            })
            .collect();
        let count = pasted.len();
        self.commit_pasted_notes((self.snap_tick(self.playback_position), 0), pasted);
        tracing::info!("Editor: 已从 Domino 剪贴板粘贴 {} 个音符", count);
        true
    }

    /// 从剪贴板读取并解析 JSON 数据，返回 (origin_key, 源 division, notes 数组)
    pub(super) fn read_clipboard_json(&self) -> Option<(u16, Option<u16>, Vec<serde_json::Value>)> {
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
    pub(super) fn parse_clipboard_notes(
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
            Some(src) if src != 0 && src != target_division => target_division as f64 / src as f64,
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
                Some(super::Note::from_raw(tick, key, length, velocity, channel))
            })
            .collect();

        Some((anchor, pasted))
    }

    /// 将解析的音符提交到编辑器并选中（O(N+M) 批量归并）
    pub(super) fn commit_pasted_notes(&mut self, _anchor: (f32, u16), pasted: Vec<super::Note>) {
        self.push_history();
        self.selection_clear();
        let pasted_count = pasted.len();
        let track = self.editor_state.data.current_track;
        // P0 修复：批量插入直接回传已分配的全局唯一 id，O(N) 完成协作广播，
        // 消除原 `note_id_at` 对每条粘贴音符做全轨线性重扫的 O(N·M) 悬崖。
        // 协作批量：100K 级粘贴改为单条批量消息（分片在 runner 侧），避免 100K 条单消息风暴。
        let ids = self.editor_state.data.batch_insert_notes_with_ids(&pasted);
        if !ids.is_empty() {
            let batch: Vec<(u64, f32, u16, f32, u8, u8, usize)> = pasted
                .iter()
                .zip(ids.iter())
                .map(|(n, id)| (*id, n.tick, n.key, n.length, n.velocity, n.channel, track))
                .collect();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_notes_added_batch(batch),
            ));
        }
        // 批量插入索引散布，旧连续选中在 tick 重叠时失效 → 按参数全等重选（最新件语义）
        self.selection_clear();
        self.select_notes_by_params(&pasted);
        self.mark_notes_changed();
        tracing::info!("Editor: 已粘贴 {} 个音符", pasted_count);
    }

    /// 遍历当前轨被选中音符（不物化索引 Vec，避免全选时 GB 级分配）
    pub(super) fn each_selected_note_on_current_track(
        &self,
        mut f: impl FnMut(&lumino_midi_loader::NoteEvent),
    ) {
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.current_track_notes();
        for &i in &interaction.selected_notes {
            if let Some(n) = notes.get(i) {
                f(n);
            }
        }
    }
}
