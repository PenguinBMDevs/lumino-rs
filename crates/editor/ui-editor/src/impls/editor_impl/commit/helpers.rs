//! 提交相关的纯辅助：多重集比较、选择广播、ghost 身份映射。

use crate::Editor;
use lumino_editor_state::DragState;
use lumino_midi_model::TickIndexedEvents;

/// 音符值多重集按份数相等（整数键计数 O(K)，无全扫）。
///
/// 键为 `(start,end,key,vel,release,chan)` 全字段；同值多份按份数比较。
pub(super) fn values_multiset_eq(
    a: &[lumino_midi_loader::NoteEvent],
    b: &[lumino_midi_loader::NoteEvent],
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut counts: std::collections::HashMap<(u32, u32, u8, u8, u8, u8), usize> =
        std::collections::HashMap::with_capacity(a.len() * 2);
    for n in a {
        *counts
            .entry((
                n.start_tick,
                n.end_tick,
                n.key,
                n.velocity,
                n.release_velocity,
                n.channel,
            ))
            .or_insert(0) += 1;
    }
    for n in b {
        let key = (
            n.start_tick,
            n.end_tick,
            n.key,
            n.velocity,
            n.release_velocity,
            n.channel,
        );
        match counts.get_mut(&key) {
            Some(c) if *c > 0 => *c -= 1,
            _ => return false,
        }
    }
    true
}

/// 广播批量移动（框选拖动提交）到协作客户端。
///
/// 批量移动与单音符拖动（`finalize_dragging`）共用同一套协作同步管线：
/// 逐音符发射 `LocalNoteMoved` 同步事件，由 Runner 转换为
/// `NoteBatchOperation(Move)` 并广播给其他客户端。`commit_pending_drag`
/// 此前完全不广播，导致协作对端无法感知框选移动——表现为「A 端移动了，
/// B 端音符状态不变」。本函数补全该缺失的广播，与移动提交一一对应。
///
/// 每个被选中音符都发射一次，携带其**原始**位置（移动前 tick/key）与本次
/// 拖动的统一偏移，对端据此匹配本地音符并叠加相同偏移完成同步。
pub(super) fn broadcast_selection_move(editor: &Editor, drag_state: &DragState) {
    // 协作同步关闭时跳过逐音符广播（消费端未连接会短路丢弃，按值）。
    if !editor.editor_state.data.collab_sync_enabled() {
        return;
    }
    let track_index = editor.editor_state.data.current_track;
    let tick_offset = drag_state.delta_tick as f32;
    let key_offset = drag_state.delta_key;
    for idx in drag_state.selected_indices() {
        // 2026-09 去 ID：原始位置取自 document 当前轨权威 NoteEvent（按值）
        let (tick, key, length) = {
            let notes = editor.editor_state.data.track_notes(track_index);
            let Some(note) = notes.get(idx) else {
                continue;
            };
            (
                note.start_tick as f32,
                note.key as u16,
                (note.end_tick - note.start_tick) as f32,
            )
        };
        lumino_message::events::emit(lumino_message::events::Event::Window(
            lumino_message::events::window::Event::local_note_moved(
                tick,
                key,
                length,
                tick_offset,
                key_offset,
                track_index,
            ),
        ));
    }
}

/// 捕获 pending 拖动的原始值快照 + 增量（窗口定位用，无全扫）。
///
/// pending 索引指向提交前 document 位置（幽灵期间 document 未变，索引有效）；
/// 若 track 已切换或索引越界，对应项跳过（份数语义：能定位多少算多少）。
pub(super) fn pending_drag_olds_of(
    editor: &Editor,
) -> (Vec<lumino_midi_loader::NoteEvent>, i64, i16) {
    match editor.transient.pending_drag.as_ref() {
        Some(ds) => {
            let track = editor.editor_state.data.current_track;
            let notes = editor.editor_state.data.track_notes(track);
            let olds: Vec<lumino_midi_loader::NoteEvent> = ds
                .selected_indices()
                .into_iter()
                .filter_map(|i| notes.get(i).copied())
                .collect();
            (olds, ds.delta_tick, ds.delta_key)
        }
        None => (Vec::new(), 0, 0),
    }
}

/// pending 原始值是否 == 当前 interaction 选中值（多重集按份数相等）。
///
/// 相等 → 保留旧框（按新值重选）；不等/空 → 已取消或新框（通用重映射）。
/// 整数键计数 O(K)，无全扫。
pub(super) fn pending_matches_interaction(
    identity: &crate::note_ops::selection_remap::SelectionIdentity,
    olds: &[lumino_midi_loader::NoteEvent],
) -> bool {
    if olds.is_empty() {
        return false;
    }
    let Some(entries) = identity.entries() else {
        // 超大选中集未逐值捕获：用基数启发（保留旧框时两者基数一致）。
        // 新框局部一般基数显著不同；误判为保留时新值重选可能遮蔽新框，
        // 但超大场景极罕见，保守选择保留跟随（宁可跟随旧框，不可错选新框？不——
        // 此处返回 false 走通用清空更安全）。为安全返回 false。
        return false;
    };
    if entries.len() != olds.len() {
        return false;
    }
    values_multiset_eq(entries, olds)
}

/// 捕获幽灵感知的选中身份：pending 选中集内的音符按视觉值（旧值 + pending delta），
/// 其余按文档旧值。供飞行中新框选在落盘后按视觉值重定位，避免框选丢失/错位。
pub(super) fn capture_ghost_aware_identity(
    editor: &Editor,
    max_key: u16,
) -> Vec<lumino_midi_loader::NoteEvent> {
    let track = editor.editor_state.data.current_track;
    let notes = editor.editor_state.data.track_notes(track);
    let pending = editor.transient.pending_drag.as_ref();
    let mut out = Vec::with_capacity(editor.editor_state.interaction.selected_notes.len());
    for i in editor.editor_state.interaction.selected_notes.iter() {
        let Some(mut ev) = notes.get(i).copied() else {
            continue;
        };
        if let Some(pp) = pending
            && i < pp.selected.len()
            && pp.selected[i]
        {
            let new_tick = (ev.start_tick as i64 + pp.delta_tick).max(0) as u32;
            let new_key = (ev.key as i32 + pp.delta_key as i32).clamp(0, max_key as i32) as u8;
            let len = ev.end_tick.saturating_sub(ev.start_tick).max(1);
            ev.start_tick = new_tick;
            ev.end_tick = new_tick.saturating_add(len);
            ev.key = new_key;
        }
        out.push(ev);
    }
    out
}

/// 按幽灵值重映射选中：清空后逐个窗口定位幽灵值（同值多份按份数分配）。
pub(super) fn remap_selection_by_ghost_identity(
    editor: &mut Editor,
    ghost_entries: &[lumino_midi_loader::NoteEvent],
) {
    if ghost_entries.is_empty() {
        if editor.editor_state.interaction.selected_notes.is_empty() {
            return;
        }
        editor.selection_clear();
        return;
    }
    editor.selection_clear();
    let mut used: std::collections::HashSet<usize> =
        std::collections::HashSet::with_capacity(ghost_entries.len().min(1024));
    let track = editor.editor_state.data.current_track;
    for ev in ghost_entries {
        if let Some(idx) = editor
            .editor_state
            .data
            .track_notes(track)
            .position_of_unused(ev, &used)
        {
            used.insert(idx);
            editor.selection_insert(idx);
        }
    }
}

/// 按新值（original + delta）重选（删加后旧值已 gone，窗口定位，无全扫）。
///
/// 先清空再逐个 `position_of` 新值；未命中者取消选中（份数语义）。
/// 同值多份按份数分配不同索引（已占用跳过），否则两个相同新值会选中同一索引丢一份。
pub(super) fn reselect_moved_news(
    editor: &mut Editor,
    olds: &[lumino_midi_loader::NoteEvent],
    delta_tick: i64,
    delta_key: i16,
    max_key: u16,
) {
    editor.selection_clear();
    let mut used: std::collections::HashSet<usize> =
        std::collections::HashSet::with_capacity(olds.len().min(1024));
    for orig in olds {
        let new_tick = (orig.start_tick as i64 + delta_tick).max(0) as u32;
        let new_key = (orig.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u8;
        let len = orig.end_tick.saturating_sub(orig.start_tick).max(1);
        let mut news = lumino_midi_loader::NoteEvent::new(
            new_tick,
            new_tick.saturating_add(len),
            new_key,
            orig.velocity,
            orig.channel,
        );
        news.release_velocity = orig.release_velocity;
        if let Some(idx) = editor
            .editor_state
            .data
            .track_notes(editor.editor_state.data.current_track)
            .position_of_unused(&news, &used)
        {
            used.insert(idx);
            editor.selection_insert(idx);
        }
    }
}
