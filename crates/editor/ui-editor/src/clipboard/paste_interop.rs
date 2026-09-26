//! 剪贴板跨视图互通粘贴（钢琴卷帘 ⇄ 工程走带）
//!
//! 从 `paste.rs` 拆出，单独承载「载荷子格式判别 + 多轨路由」职责。
//! 拆分原因：`paste.rs` 加入互通逻辑后超过 400 行红线，本模块与其是
//! 正交的关注点（前者 = 载荷落地与选区，后者 = 载荷解码与跨轨映射）。
//!
//! # 子格式判别（唯一的分叉点）
//!
//! | 载荷 | 二进制 `track_hint` | JSON `type` | per-note track 语义 | 落点 |
//! |---|---|---|---|---|
//! | 钢琴卷帘 | 真实轨号 | 无 | 偏移，恒 0 | 当前轨（单轨快路径） |
//! | 工程走带 | `ARRANGEMENT_BINARY_MARK` | `"arrangement"` | 相对锚点的视觉轨偏移 | `锚点视觉轨 + 偏移` |
//!
//! # P0 修复（互通前的两个静默/静错缺陷）
//!
//! 1. 走带粘贴硬拒非哨兵二进制 + 非 `type=arrangement` JSON → 「卷帘复制 →
//!    走带粘贴」**静默失败**（仅一条 warn 日志，用户看不到任何提示）。
//! 2. 卷帘粘贴把 per-note track 字段命名 `_track_hint` **直接丢弃** → 「走带
//!    复制 → 卷帘粘贴」把所有音轨音符**叠进当前轨**：看似成功、结果全错，
//!    比静默失败更危险（用户会以为粘贴对了）。

use super::*;
use crate::clipboard::ARRANGEMENT_BINARY_MARK;
use std::collections::HashMap;

impl Editor {
    /// 二进制载荷粘贴（纯函数，不触碰系统剪贴板，便于单元测试）。
    ///
    /// # 跨视图互通（2026-09）
    ///
    /// 原实现把 `decode_clipboard_records` 回调的 per-note track 字段命名为
    /// `_track_hint` **直接丢弃**，所有音符一律插入当前轨——走带复制的多轨内容
    /// 粘贴后会全部叠进当前轨（**看似成功、结果全错**，比静默失败更危险）。
    ///
    /// 现在按 [`ARRANGEMENT_BINARY_MARK`] 判别子格式：
    /// - 走带子格式：按 per-note 视觉偏移做**多轨路由**（`dest_visual = 锚点视觉轨 +
    ///   偏移`，再经 `document_track_at` 映射回文档轨），与走带粘贴逐字节一致；
    /// - 卷帘子格式：偏移恒 0（见 [`Self::build_clipboard_binary`]），走原单轨快路径，
    ///   **行为完全不变**（零回归）。
    #[cfg(windows)]
    pub(crate) fn paste_binary_payload(&mut self, bytes: &[u8]) -> bool {
        let meta = match parse_clipboard_header(bytes) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Editor: 二进制剪贴板头解析失败: {e}");
                return false;
            }
        };
        let multi_track = meta.track_hint == ARRANGEMENT_BINARY_MARK;
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
        let current_track = self.editor_state.data.current_track;
        let anchor_visual = self
            .editor_state
            .data
            .visual_position_of(current_track)
            .unwrap_or(current_track);
        let max_visual = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(0);

        // 走带子格式：多轨路由（按目标轨分组）
        // 卷帘子格式：单轨快路径
        let mut by_track: HashMap<usize, Vec<Note>> = HashMap::new();
        let mut single: Vec<Note> = Vec::new();
        let mut total = 0usize;
        let decode_res = decode_clipboard_records(
            bytes,
            |tick_offset, length, key_offset, velocity, channel, track_offset| {
                let to = if ratio == 1.0 {
                    tick_offset as f64
                } else {
                    (tick_offset as f64 * ratio).round()
                };
                let le = if ratio == 1.0 {
                    length as f64
                } else {
                    (length as f64 * ratio).round()
                };
                let tick = (anchor_tick + to as f32).max(0.0);
                let key =
                    (meta.origin_key as i32 + key_offset as i32).clamp(0, max_key as i32) as u8;
                let note = Note::from_raw(tick, key as u16, le as f32, velocity, channel);
                total += 1;
                if multi_track {
                    let dest_visual = (anchor_visual as i64 + track_offset as i64).max(0) as usize;
                    if dest_visual >= max_visual {
                        return;
                    }
                    let dest_doc = self.editor_state.data.document_track_at(dest_visual);
                    by_track.entry(dest_doc).or_default().push(note);
                } else {
                    single.push(note);
                }
            },
        );
        if decode_res.is_err() || total == 0 {
            return false;
        }

        self.push_history();
        self.selection_clear();

        if multi_track {
            self.insert_grouped_notes(by_track);
            self.mark_notes_changed();
            tracing::info!(
                "Editor: 已从二进制剪贴板多轨粘贴 {} 个音符（走带子格式）",
                total
            );
            return true;
        }

        let inserted = single.len();
        self.commit_pasted_notes((anchor_tick, 0), single);
        tracing::info!("Editor: 已从二进制剪贴板粘贴 {} 个音符", inserted);
        true
    }

    /// 按目标文档轨分组批量插入（跨轨路由用）
    ///
    /// 无平台依赖（JSON 路径在所有平台都需要它），故**不带 `cfg(windows)`**——
    /// 带的话非 Windows 构建下 `paste_json_payload` 会找不到本函数。
    ///
    /// 选区不在此处设置：卷帘选区是单轨索引位图，无法表达跨轨选择；走带侧由
    /// [`Self::apply_clipboard_paste_entries`] 统一冻结新粘贴音符。
    fn insert_grouped_notes(&mut self, by_track: HashMap<usize, Vec<Note>>) -> usize {
        let current_track = self.editor_state.data.current_track;
        let collab_sync = self.editor_state.data.collab_sync_enabled();
        let mut inserted = 0usize;
        let mut batch_acc: Vec<(f32, u16, f32, u8, u8, usize)> = Vec::new();
        for (dest_track, notes) in by_track {
            let _ids = self
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(dest_track, &notes);
            for note in notes.iter() {
                inserted += 1;
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
            if dest_track == current_track {
                self.mark_notes_changed();
            }
            self.editor_state
                .data
                .mark_track_notes_changed_for(Some(HashSet::from([dest_track])));
        }
        if !batch_acc.is_empty() {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_notes_added_batch(batch_acc),
            ));
        }
        inserted
    }

    /// JSON 载荷粘贴（纯函数，不触碰系统剪贴板，便于单元测试）。
    ///
    /// 走带格式（`type="arrangement"`）按 per-note `"track"` 视觉偏移多轨路由；
    /// 卷帘格式（无 `type`）偏移恒 0，走原单轨快路径，行为完全不变。
    pub(crate) fn paste_json_payload(&mut self, text: &str) -> bool {
        let Some((origin_key, source_division, notes_value)) = self.parse_json_payload(text) else {
            return false;
        };
        let multi_track = self
            .parse_json_payload_is_arrangement(text)
            .unwrap_or(false);

        if !multi_track {
            // 卷帘子格式：原单轨路径（行为零变化）
            if let Some((anchor, pasted)) =
                self.parse_clipboard_notes(origin_key, source_division, &notes_value)
                && !pasted.is_empty()
            {
                self.commit_pasted_notes(anchor, pasted);
                return true;
            }
            return false;
        }

        // 走带子格式：多轨路由
        let target_div = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        let ratio = match source_division {
            Some(src) if src != 0 && src != target_div => target_div as f64 / src as f64,
            _ => 1.0,
        };
        let anchor_tick = self.snap_tick(self.playback_position);
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let current_track = self.editor_state.data.current_track;
        let anchor_visual = self
            .editor_state
            .data
            .visual_position_of(current_track)
            .unwrap_or(current_track);
        let max_visual = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(0);

        let mut by_track: HashMap<usize, Vec<Note>> = HashMap::new();
        for item in &notes_value {
            let (Some(raw_tick), Some(raw_length), Some(key_offset)) = (
                item.get("tick").and_then(|v| v.as_f64()),
                item.get("length").and_then(|v| v.as_f64()),
                item.get("key").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
            let channel = item.get("channel").and_then(|c| c.as_u64()).unwrap_or(0) as u8;
            let track_offset = item
                .get("track")
                .and_then(|t| t.as_i64())
                .unwrap_or(0)
                .max(0) as usize;
            let dest_visual = anchor_visual + track_offset;
            if dest_visual >= max_visual {
                continue;
            }
            let dest_doc = self.editor_state.data.document_track_at(dest_visual);
            let tick_offset = if ratio == 1.0 {
                raw_tick as f32
            } else {
                (raw_tick * ratio).round() as f32
            };
            let length = if ratio == 1.0 {
                raw_length as f32
            } else {
                (raw_length * ratio).round() as f32
            };
            let key = origin_key.saturating_add(key_offset as u16).min(max_key);
            by_track.entry(dest_doc).or_default().push(Note::from_raw(
                (anchor_tick + tick_offset).max(0.0),
                key,
                length,
                velocity,
                channel,
            ));
        }
        if by_track.is_empty() {
            return false;
        }
        self.push_history();
        self.selection_clear();
        self.insert_grouped_notes(by_track);
        self.mark_notes_changed();
        tracing::info!("Editor: 已从 JSON 剪贴板多轨粘贴音符");
        true
    }
}
