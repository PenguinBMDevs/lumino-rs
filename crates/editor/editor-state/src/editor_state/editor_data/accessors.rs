//! 访问器 —— 音符读写访问器 + 增量事件记录
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `document`（MidiDocument），
//! 所有读取/写入经本模块访问器。tick 精度：UI 编辑用 f32，写回时
//! 无损转换（`fract() == 0.0` 直接 as u32），异常亚 tick 才 round + warn。

use std::collections::HashSet;

use super::EditorData;
use lumino_midi_model::NoteEvent;
use lumino_note_core::note::Note;

impl EditorData {
    /// 标记音符数据已变化（递增版本号）
    ///
    /// 所有直接修改音符数据的地方都必须在操作后调用此方法，
    /// 否则 NoteWorker 快照缓存无法感知数据变化。
    ///
    /// 变化来源未知或影响全部音轨（`onion_dirty_tracks = None`），
    /// 洋葱皮会保守执行全量重建。调用方若能明确受影响音轨，
    /// 请使用 [`Self::mark_track_notes_changed_for`] 以获得增量豁免。
    #[inline]
    pub fn mark_track_notes_changed(&mut self) {
        self.mark_track_notes_changed_for(None);
    }

    /// 标记音符数据已变化，并记录明确受影响的音轨集合
    ///
    /// `tracks` 为本次操作实际修改的音轨 id 集合。当集合全部落在
    /// 洋葱皮跳过范围（当前音轨 / 静音音轨）时，可豁免全量重建上传。
    /// `None` 表示未知或影响全部音轨（保守语义，同 [`Self::mark_track_notes_changed`]）。
    ///
    /// 主音轨增量对账（2026-08-05）：当前音轨变化默认置 `note_delta_dirty = true`
    /// （未记录事件 → 渲染层全量兜底）。仅其他音轨变化不影响主音轨数据，
    /// 不置 dirty（洋葱皮编辑不牵连主音轨增量路径）。
    #[inline]
    pub fn mark_track_notes_changed_for(&mut self, tracks: Option<HashSet<usize>>) {
        self.onion_dirty_tracks = tracks;
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
        match &self.onion_dirty_tracks {
            Some(t) if !t.contains(&self.current_track) => {}
            _ => self.note_delta_dirty = true,
        }
    }

    /// 标记当前音轨的音符已变化（热路径专用）
    ///
    /// 编辑操作绝大多数作用于当前音轨（拖动音符、增删改），而洋葱皮
    /// 不显示当前音轨——精确记录音轨 id 后，洋葱皮可豁免全量重建上传，
    /// 避免「拖动主音轨音符 → 每帧全量重传其他所有音轨」的冗余。
    #[inline]
    pub fn mark_current_track_changed(&mut self) {
        let current_track = self.current_track;
        self.mark_track_notes_changed_for(Some(HashSet::from([current_track])));
    }

    /// 返回文档音轨索引对应的视觉位置
    ///
    /// 侧边栏音轨按原始序号排列，视觉位置与文档音轨索引一致（恒等映射）。
    /// 此方法保留供 arrangement 操作统一使用，便于未来支持拖动排序等变化。
    ///
    /// 如果音轨不在映射中，返回 `None`（此时回退到 `track_idx` 本身作为视觉位置）。
    pub fn visual_position_of(&self, track_id: usize) -> Option<usize> {
        self.track_visual_order
            .iter()
            .position(|&id| id == track_id)
    }

    /// 返回视觉位置对应的文档音轨索引（与 [`Self::visual_position_of`] 互逆）
    ///
    /// 侧边栏顺序即视觉顺序（拖动排序后 `track_visual_order` 同步更新）。
    /// 音轨不在映射中时回退到恒等映射（视觉位置即文档索引），
    /// 保证未初始化/部分同步状态下不会越界访问。
    #[inline]
    pub fn document_track_at(&self, visual_pos: usize) -> usize {
        self.track_visual_order
            .get(visual_pos)
            .copied()
            .unwrap_or(visual_pos)
    }

    /// 取走主音轨增量事件队列（UI 层每帧消费）
    #[inline]
    pub fn take_note_delta_events(
        &mut self,
    ) -> Vec<crate::editor_state::editor_data::NoteDeltaEvent> {
        std::mem::take(&mut self.note_delta_events)
    }

    // ── 音符读取（document 唯一权威） ─────────────────────────

    /// 获取当前轨道音符的分块引用（零拷贝，直接借自 document）
    ///
    /// 无 document 或音轨不存在时返回空容器引用。
    #[inline]
    pub fn current_track_notes(&self) -> &lumino_midi_model::ChunkedList<NoteEvent> {
        self.track_notes(self.current_track)
    }

    /// 获取指定音轨音符的分块引用（零拷贝，直接借自 document）
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &lumino_midi_model::ChunkedList<NoteEvent> {
        static EMPTY: lumino_midi_model::ChunkedList<NoteEvent> =
            lumino_midi_model::ChunkedList::EMPTY;
        self.document
            .as_ref()
            .map(|doc| doc.track_notes(track_id))
            .unwrap_or(&EMPTY)
    }

    /// 当前轨道音符数量（无 document 时为 0）
    #[inline]
    pub fn current_track_note_count(&self) -> usize {
        self.document
            .as_ref()
            .map(|doc| doc.track_note_count(self.current_track as u16) as usize)
            .unwrap_or(0)
    }

    // ── 音符写入（document 唯一权威） ─────────────────────────

    /// 在指定音轨按 start_tick 升序插入音符（f32 tick 无损转换写回）。
    ///
    /// 返回是否插入成功（音轨不存在返回 false）。调用方需在调用前 `push_history()`。
    pub fn insert_note(&mut self, track_id: usize, note: Note) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        let event = note_to_event(note);
        doc.insert_note(track_id, event)
    }

    /// 确保指定音轨存在（不存在则自动扩轨，图片转 MIDI 自动建轨用）。
    /// document 为空时返回 false。
    pub fn ensure_track(&mut self, track_id: usize) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        while doc.track_count() <= track_id {
            doc.add_empty_track();
        }
        true
    }

    /// 在指定音轨指定索引处删除音符。返回被删除的音符。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        self.document.as_mut()?.remove_note(track_id, index)
    }

    /// 替换指定音轨指定索引处的音符（内部按序重新插入，保持升序不变式）。
    pub fn update_note(&mut self, track_id: usize, index: usize, note: Note) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        doc.update_note(track_id, index, note_to_event(note))
    }

    /// 整轨替换音符（undo/redo 快照恢复专用）。
    pub fn replace_track_notes(&mut self, track_id: usize, notes: Vec<NoteEvent>) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        doc.replace_track_notes(track_id, notes)
    }

    /// 整轨替换音符（undo/redo 快照恢复专用，O(块数) 浅拷贝版）。
    ///
    /// 直接共享快照块 Arc，不做数据复制（1600W 音符工程 undo/redo 免整轨拷贝）。
    pub fn replace_track_notes_chunked(
        &mut self,
        track_id: usize,
        notes: &lumino_midi_model::ChunkedList<NoteEvent>,
    ) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        doc.replace_track_notes_chunked(track_id, notes)
    }

    // ── 增量事件记录 ─────────────────────────────────────────

    /// 记录等长修改增量事件（整轨同步版）
    ///
    /// 将 `indices`（修改的 notes 索引，无序可重复）合并为连续区间
    /// `UpdateRange` 事件，随后标记变化并清除 dirty（事件已完整记录）。
    ///
    /// 供 EditorTransform（变速/翻转/移调/批量编辑）等整轨同步路径使用。
    pub fn record_update_ranges(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.mark_current_track_changed();
        self.note_delta_dirty = false;
    }

    /// 记录等长修改增量事件（流式同步版，拖动热路径）
    ///
    /// 与 [`Self::record_update_ranges`] 相同的事件记录，供
    /// `apply_drag_state_streaming` 使用。
    pub fn record_update_ranges_streamed(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.mark_current_track_changed();
        self.note_delta_dirty = false;
    }

    /// 将升序去重后的索引合并为连续区间事件（纯数据操作，不同步）
    fn push_update_range_events(&mut self, indices: &[usize]) {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.is_empty() {
            return;
        }
        let mut start = sorted[0];
        let mut prev = sorted[0];
        for &i in &sorted[1..] {
            if i == prev + 1 {
                prev = i;
                continue;
            }
            self.push_update_range(start, prev);
            start = i;
            prev = i;
        }
        self.push_update_range(start, prev);
    }

    /// 推送单个连续区间事件（越界索引防御性过滤）
    fn push_update_range(&mut self, start: usize, end: usize) {
        let notes: Vec<Note> = self
            .current_track_notes()
            .iter()
            .skip(start)
            .take(end - start + 1)
            .map(event_to_note)
            .collect();
        if !notes.is_empty() {
            self.note_delta_events.push(
                crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                    start_index: start,
                    notes,
                },
            );
        }
    }
}

/// Note（f32 tick）→ NoteEvent（u32 tick）无损转换
///
/// UI 编辑的 tick 全部来自 `snap_tick`（整数网格吸附），正常路径 `fract() == 0.0`。
/// 异常亚 tick（防御性）使用 round 并记录 warn——不引入架构性精度损失。
#[inline]
pub fn note_to_event(note: Note) -> NoteEvent {
    let start_tick = f32_to_tick(note.tick);
    let end_tick = f32_to_tick(note.tick + note.length);
    NoteEvent::new(
        start_tick,
        end_tick,
        note.key as u8,
        note.velocity,
        note.channel,
    )
}

/// NoteEvent（u32 tick）→ Note（f32 tick）无损转换
#[inline]
pub fn event_to_note(event: &NoteEvent) -> Note {
    Note::from_raw(
        event.start_tick as f32,
        event.key as u16,
        (event.end_tick - event.start_tick) as f32,
        event.velocity,
        event.channel,
    )
}

/// f32 tick → u32 tick：无损优先（fract==0），异常亚 tick round + trace
///
/// 注意：非整数 tick 是**设计内预期行为**（图片转 MIDI 区域等比映射会生成
/// 亚 tick 数值），round 是防御性兜底而非异常，因此仅 trace 不 warn——
/// 否则 i2m 批量写入时每音符一条 WARN，高频日志格式化 + 终端 I/O 阻塞主线程。
#[inline]
pub fn f32_to_tick(tick: f32) -> u32 {
    if tick.fract() == 0.0 {
        tick as u32
    } else {
        tracing::trace!("非整数 tick 写回 MIDI: {tick}，已四舍五入");
        tick.round() as u32
    }
}
