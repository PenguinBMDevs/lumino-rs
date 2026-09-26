use super::*;
use std::collections::HashMap;

impl Editor {
    /// 多轨批量粘贴内核（钢琴卷帘 / 工程走带**共用同一实现**）。
    ///
    /// 把 `entries` 按目标文档轨分组后逐轨 `batch_insert_notes_to_track_with_ids`
    /// 一次性插入，并返回受影响音轨集合。调用方负责在此之前 `push_history()`、
    /// 在插入数为 0 时 `discard_last_history()`。
    ///
    /// **P0 修复（统一）**：原实现是走带私有的 `apply_paste_internal`，钢琴卷帘
    /// 无法复用（其粘贴路径逐音符 `insert_note`）。两条路径的插入语义、脏标记、
    /// 协作广播完全一致才是「两个视图操作方式统一」的实质——故提升为公共内核。
    ///
    /// P0 修复（去 ID 悬崖）：按目标轨分组批量插入（O(N·log M)），并直接拿回已分配
    /// id 广播，消除原逐条 `insert_note`（O(N·M)）+ `note_id_at`（O(N·M)）双重悬崖。
    pub(crate) fn apply_clipboard_paste_entries(
        &mut self,
        anchor_tick: f32,
        origin_key: u16,
        entries: &[ClipboardNoteEntry],
        collect_selection: bool,
    ) -> PasteOutcome {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut inserted_count = 0usize;
        let mut affected_tracks: HashSet<usize> = HashSet::new();
        let mut frozen_entries: Vec<(u16, u32, u32, u8)> = Vec::new();

        // 按目标文档音轨分组，批量插入并直接取回已分配 id
        let mut by_track: HashMap<usize, Vec<Note>> = HashMap::new();
        for (dest_doc, tick_offset, key_offset, length, velocity, channel) in entries {
            let note_tick = (anchor_tick + *tick_offset).max(0.0);
            let note_key = origin_key.saturating_add(*key_offset).min(127);
            let note = Note::from_raw(note_tick, note_key, *length, *velocity, *channel);
            by_track.entry(*dest_doc).or_default().push(note);
        }

        puffin::profile_scope!("clipboard::insert_notes");
        let t0 = std::time::Instant::now();
        let collab_sync = self.editor_state.data.collab_sync_enabled();
        let mut batch_acc: Vec<(f32, u16, f32, u8, u8, usize)> = Vec::new();
        for (dest_track, notes) in by_track {
            let visual = self
                .editor_state
                .data
                .visual_position_of(dest_track)
                .unwrap_or(dest_track) as u16;
            let _ids = self
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(dest_track, &notes);
            for note in notes.iter() {
                affected_tracks.insert(dest_track);
                if dest_track == current_track {
                    current_track_touched = true;
                }
                inserted_count += 1;
                if collect_selection {
                    // 冻结条目用文档权威 tick（f32_to_tick），否则 contains 命不中
                    frozen_entries.push((
                        visual,
                        lumino_editor_state::f32_to_tick(note.tick),
                        lumino_editor_state::f32_to_tick(note.tick + note.length),
                        note.key.min(u8::MAX as u16) as u8,
                    ));
                }
                // 协作同步关闭时不构建批量广播载荷（按值）。
                if collab_sync {
                    batch_acc.push((
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
        // 协作批量：粘贴统一走批量消息（协作同步关闭时不发射）
        if !batch_acc.is_empty() {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_notes_added_batch(batch_acc),
            ));
        }
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tracing::debug!(
            target: "perf::clipboard",
            inserted = inserted_count,
            ms = elapsed_ms,
            "insert_notes"
        );

        PasteOutcome {
            inserted: inserted_count,
            current_track_touched,
            affected_tracks,
            frozen_entries,
        }
    }

    /// 解析 JSON 载荷，返回 `(origin_key, 源 division, notes 数组)`
    pub(super) fn parse_json_payload(
        &self,
        text: &str,
    ) -> Option<(u16, Option<u16>, Vec<serde_json::Value>)> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        let division = value
            .get("division")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);
        let notes = value.get("notes")?.as_array()?.to_vec();
        Some((origin_key, division, notes))
    }

    /// 载荷是否为走带子格式（`type == "arrangement"`）
    pub(super) fn parse_json_payload_is_arrangement(&self, text: &str) -> Option<bool> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(value.get("type").and_then(|t| t.as_str()) == Some("arrangement"))
    }

    /// 二进制私有格式粘贴（Windows）。
    ///
    /// 流式解码为 `NoteEvent`（升序）→ 锚点定位 → 含 PPQN 重采样的**单次批量插入**。
    /// 2026-09 性能修复：原逐 100K 块插入对增长中轨道每次归并 O(轨道+块)，
    /// 总量级 O(N·块数)（2M 音符 ≈ 58M 元素搬移）；改为全量单次归并 O(N+M)，
    /// 且 `decode_clipboard_records` 免 `ClipRecord`/`Note` 两层中间物化。
    /// 峰值内存为「新事件 16B/音符 + 单次归并输出块」，2M 音符约 120MB。
    #[cfg(windows)]
    pub(super) fn try_paste_from_binary(&mut self) -> bool {
        let Some(bytes) = crate::clipboard::sys::get_clipboard_binary() else {
            return false;
        };
        self.paste_binary_payload(&bytes)
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

    /// 从 JSON 载荷解析锚点坐标和音符列表，并按 PPQN 一致性重采样。
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

    /// 将解析的音符提交到编辑器并选中（O(N+M) 批量归并，按值广播）
    pub(super) fn commit_pasted_notes(&mut self, _anchor: (f32, u16), pasted: Vec<super::Note>) {
        self.push_history();
        self.selection_clear();
        let pasted_count = pasted.len();
        let track = self.editor_state.data.current_track;
        // P0 修复（去 ID）：批量插入按值完成协作广播，无全轨重扫悬崖。
        // 协作批量：100K 级粘贴改为单条批量消息（分片在 runner 侧），避免 100K 条单消息风暴。
        // 协作同步关闭时不构建载荷（消费端未连接会短路丢弃）。
        let ids = self.editor_state.data.batch_insert_notes_with_ids(&pasted);
        if !ids.is_empty() && self.editor_state.data.collab_sync_enabled() {
            let batch: Vec<(f32, u16, f32, u8, u8, usize)> = pasted
                .iter()
                .map(|n| (n.tick, n.key, n.length, n.velocity, n.channel, track))
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
        for i in &interaction.selected_notes {
            if let Some(n) = notes.get(i) {
                f(n);
            }
        }
    }
}
