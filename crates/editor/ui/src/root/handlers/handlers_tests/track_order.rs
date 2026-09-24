//! 轨道有序不变式回归（工具栏量化路径）
//!
//! 背景：量化子集就地改 tick，被量化音符可越过未量化音符的 tick → 破坏
//! 「按 start_tick 升序」不变式（`window_range` 二分漏检 → 音符不渲染/不可点）。
//! 修复：量化后 `restore_current_track_sorted` 恢复，重排时主轨全量重建。

use super::*;
use crate::editor::note::Note;
use crate::toolbar::Event as ToolbarEvent;

#[test]
fn test_quantize_subset_restores_track_order() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = create_root();
    attach_test_document(&mut root);

    // 量化网格与处理器同源计算（默认 zoom_x/ppq）
    let view = &root.editor.editor_state.view;
    let grid = crate::editor::grid::utils::adaptive_grid_gap(view.zoom_x, view.ppq as f32);
    assert!(grid > 4.0, "量化网格应有效");
    // 选中 S = grid/2（四舍五入向上到 grid），未选中 U = 3/4 grid（位于 S 与 grid 之间）
    let s_tick = (grid * 0.5).round();
    let u_tick = (grid * 0.75).round();
    root.editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(s_tick, 60, 60.0, 100, 0));
    root.editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(u_tick, 62, 60.0, 100, 0));
    root.editor.selection_insert(0); // 只选 S

    let mut handler = ToolbarHandler::new();
    handler.handle(&mut root, Message::Toolbar(ToolbarEvent::Quantize));

    // S 量化到 grid（越过 U）→ 必须重排恢复升序
    let ticks: Vec<u32> = root
        .editor
        .editor_state
        .data
        .track_notes(1)
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert!(
        ticks.windows(2).all(|w| w[0] <= w[1]),
        "量化越过未选中音符后必须恢复升序: {ticks:?}"
    );
    assert!(
        ticks.contains(&(grid as u32)),
        "S 应量化到网格点 {grid}: {ticks:?}"
    );
    assert!(
        root.editor.editor_state.data.note_delta_dirty,
        "重排 → 主轨全量重建"
    );
}
