//! NoteRectangle 模式：传统钢琴卷帘音符矩形

use lumino_gfx::RenderParams;

use super::{
    NoteRectangleParamsInput, NoteRectangleRenderInput, SortableNote,
    build_note_rectangle_params_from_visible, note_search_bounds,
};

/// NoteRectangle 模式：传统 GPU 音符矩形渲染
pub(crate) fn build_note_rectangle_render_params(input: NoteRectangleRenderInput) -> RenderParams {
    let NoteRectangleRenderInput {
        width,
        height,
        tick,
        document,
        ppq,
        visible_notes,
        note_instances_out,
    } = input;
    // 视频导出始终使用标准 128 键 MIDI 键盘
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    // 每轨按 start_tick 有序 → 二分窗口定位，避免每帧 O(N) 全量遍历
    visible_notes.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in track_notes.iter().take(search_end) {
            if n.end_tick >= tick_start && n.start_tick <= tick_end {
                visible_notes.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.length(),
                    track_idx: track_idx as u16,
                });
            }
        }
    }

    // 排序分桶 + NoteInstance 构建 + RenderParams 组装（与流式模式共享）
    build_note_rectangle_params_from_visible(NoteRectangleParamsInput {
        width,
        height,
        tick,
        visible_notes,
        note_instances_out,
        ppq,
        time_signatures: &document.time_signatures,
    })
}
