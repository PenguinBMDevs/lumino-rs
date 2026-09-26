//! 基准操作实现：与 UI 层生产路径同构的复制/粘贴/移动构造（由主文件拆出）

use lumino_midi_loader::NoteEvent;
use lumino_midi_model::clipboard::{decode_clipboard_records, parse_clipboard_header};
use lumino_note_core::history::MoveOp;
use lumino_ui_editor::Editor;

/// 复制：直接调用生产编码路径 `Editor::build_clipboard_binary`
/// （紧凑二进制、流式编码、全选快路径），保证基准与实现不漂移。
pub fn encode_selected(editor: &Editor) -> Vec<u8> {
    let track = editor.editor_state.data.current_track;
    let division = editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|d| d.division)
        .unwrap_or(480);
    editor
        .build_clipboard_binary(track, division)
        .unwrap_or_default()
}

/// 粘贴：与 `Editor::try_paste_from_binary` 同构（流式解码 + **单次**批量插入 + push_history）。
///
/// 不含系统剪贴板读写与消息层广播（消息层消费端未连接时短路丢弃，属独立层）。
pub fn paste_payload(editor: &mut Editor, bytes: &[u8]) -> usize {
    let meta = parse_clipboard_header(bytes).expect("剪贴板头解析失败");
    let target_div = editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|d| d.division)
        .unwrap_or(480);
    let ratio = if meta.division != 0 && meta.division != target_div {
        target_div as f64 / meta.division as f64
    } else {
        1.0
    };
    let anchor_tick = editor.playback_position;
    let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);
    let track = editor.editor_state.data.current_track;

    // 与生产粘贴路径一致：先入历史快照，再清空选择执行插入
    editor.push_history();
    editor.selection_clear();

    // 单次归并：全量流式解码升序 NoteEvent 后一次批量插入（O(N+M)，免逐块重复归并）
    let mut events: Vec<NoteEvent> = Vec::with_capacity(meta.count as usize);
    decode_clipboard_records(
        bytes,
        |tick_offset, length, key_offset, velocity, channel, _track_hint| {
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
            let key = (meta.origin_key as i32 + key_offset as i32).clamp(0, max_key as i32) as u8;
            events.push(NoteEvent::new(
                lumino_editor_state::f32_to_tick(tick),
                lumino_editor_state::f32_to_tick(tick + le as f32),
                key,
                velocity,
                channel,
            ));
        },
    )
    .expect("剪贴板解码失败");

    let total = events.len();
    // 与生产粘贴路径一致：未连接协作时不回收 id 广播列表
    editor
        .editor_state
        .data
        .batch_insert_events_to_track(track, events);
    editor.mark_notes_changed();
    total
}

/// 构造按值 `MoveOp`（捕获当前轨全部音符的原始快照，与拖动提交同构）。
pub fn build_move_ops(editor: &Editor) -> Vec<MoveOp> {
    let data = &editor.editor_state.data;
    let track = data.current_track;
    let notes = data.track_notes(track);
    let originals: Vec<NoteEvent> = notes.iter().copied().collect();
    vec![MoveOp {
        track_id: track as u32,
        originals,
        delta_tick: 12,
        delta_key: 1,
        seq: 0,
    }]
}

/// 排空事件总线（模拟 UI 每帧 drain；不计时、不计入操作口径）。
pub fn drain_events() {
    let events = lumino_message::events::take_events();
    std::hint::black_box(events.len());
}

/// 轨道身份：长度 + 首尾音符 (tick, key)
pub type Identity = (usize, Option<(u32, u8)>, Option<(u32, u8)>);

/// 轨道身份校验：长度 + 首尾音符 (tick, key)。用于确认操作后数据无损。
pub fn track_identity(editor: &Editor) -> Identity {
    let notes = editor.editor_state.data.current_track_notes();
    let first = notes.first().map(|n| (n.start_tick, n.key));
    let last = notes.last().map(|n| (n.start_tick, n.key));
    (notes.len(), first, last)
}
