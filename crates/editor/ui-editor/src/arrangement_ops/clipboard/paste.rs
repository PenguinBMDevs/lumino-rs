//! 走带剪贴板粘贴：文本/二进制载荷解析、锚点计算与批量落轨

use crate::Editor;
use crate::clipboard::ClipboardNoteEntry;
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
    /// Windows 下优先尝试紧凑二进制（任意 Lumino 子格式，毫秒级）；
    /// 未命中则退化为 JSON 文本路径（跨平台正确）。
    ///
    /// 跨视图互通：钢琴卷帘复制的载荷同样可被粘贴（`track` 偏移恒 0 → 落锚点轨）。
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

        let inserted_count = self.apply_paste_internal(anchor_tick, origin_key, &pasted);

        if inserted_count == 0 {
            self.editor_state.data.discard_last_history();
            return false;
        }
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
    ///
    /// # 跨视图互通（2026-09）
    ///
    /// 原实现在此硬拒 `track_hint != ARRANGEMENT_BINARY_MARK`，导致「钢琴卷帘复制 →
    /// 走带粘贴」**静默失败**（只打一条 warn，用户看不到任何提示）。现在统一接受
    /// 任意 Lumino 二进制子格式：`ClipRecord.track` 已统一为「相对锚点的视觉轨偏移」
    /// 语义（卷侧恒 0），故卷帘载荷按偏移 0 落锚点轨，天然正确，无需分支。
    ///
    /// `track_hint` 仅用于日志标注载荷来源，不再参与准入判定。
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
        tracing::debug!(
            "Arrangement: 读取二进制剪贴板载荷（track_hint={}，{} 音符，division={}）",
            meta.track_hint,
            meta.count,
            meta.division
        );
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
        let inserted_count = self.apply_paste_internal(anchor_tick, origin_key, &pasted);
        if inserted_count == 0 {
            self.editor_state.data.discard_last_history();
            return false;
        }
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
    ///
    /// # `type` 白名单（跨视图互通）
    ///
    /// 接受 `None` / `"notes"` / `"arrangement"` 三种：钢琴卷帘 JSON 不带 `type`
    /// 字段（历史格式），走带 JSON 带 `type="arrangement"`。原先只接受后者，导致
    /// 「卷帘复制 → 走带粘贴」静默失败。卷帘载荷的每音符 `track` 偏移恒为 0
    /// （见 `clipboard/encode.rs`），故按偏移路由时天然落到锚点轨，无需特判。
    fn parse_clipboard_json_text(
        &self,
        text: &str,
    ) -> Option<(u16, usize, Option<u16>, Vec<serde_json::Value>)> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;

        let clipboard_type = value.get("type").and_then(|t| t.as_str());
        let origin_key = value.get("origin_key")?.as_u64()? as u16;
        // `origin_track`：载荷的锚点视觉轨（走带格式自描述字段）。
        // 钢琴卷帘 JSON 不写该字段（其单轨语义下锚点恒为当前轨）——故**可选**，
        // 缺失时按 0 处理。实际粘贴锚点一律由 `compute_anchor_visual()` 从当前
        // 选区 / 当前轨现算（见 `parse_arrangement_clipboard_notes`），
        // 本字段仅供格式自描述与未来扩展，不参与落点计算。
        let origin_track = value
            .get("origin_track")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        // 源 division（PPQN），用于粘贴端 PPQN 一致性重采样；缺失则视为与目标一致。
        let division = value
            .get("division")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);
        let notes = value.get("notes")?.as_array()?.to_vec();

        // 白名单：`None`（卷帘历史格式）/ `"notes"`（卷帘显式格式）/ `"arrangement"`（走带）
        if matches!(clipboard_type, None | Some("notes") | Some("arrangement")) {
            Some((origin_key, origin_track, division, notes))
        } else {
            tracing::warn!(
                "Arrangement: 剪贴板数据不是 Lumino 音符格式 (type={:?})",
                clipboard_type
            );
            None
        }
    }

    /// 执行粘贴：调用公共多轨内核 + 标记脏 + **冻结选区为新粘贴音符**。
    ///
    /// `pasted` 中的 `dest_track` 已由解析阶段映射为文档音轨索引（视觉偏移经
    /// `document_track_at` 转换），此处直接插入。返回实际插入数（0 = 未粘贴）。
    ///
    /// **P0 修复（粘贴后选区跟随）**：粘贴成功后把选区冻结为**本次新粘贴的音符**。
    /// 原实现完全不更新选区，导致连续 Ctrl+V 的锚点（`compute_anchor_visual` 读旧
    /// 矩形）不可预测——卷帘侧 `commit_pasted_notes` 一直是「粘贴即选中」，
    /// 两视图行为统一到同一语义。
    ///
    /// 脏标记与协作广播统一收口于此（调用方不再重复标记）。
    fn apply_paste_internal(
        &mut self,
        anchor_tick: f32,
        origin_key: u16,
        pasted: &[ClipboardNoteEntry],
    ) -> usize {
        let outcome = self.apply_clipboard_paste_entries(anchor_tick, origin_key, pasted, true);

        if outcome.current_track_touched {
            self.mark_notes_changed();
        }
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(outcome.affected_tracks));

        // 粘贴即选中：选区冻结为新粘贴音符（精确集合，语义同卷帘 select_notes_by_params）
        if !outcome.frozen_entries.is_empty() {
            self.editor_state
                .data
                .arrange_selection
                .freeze(outcome.frozen_entries);
        }

        outcome.inserted
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
