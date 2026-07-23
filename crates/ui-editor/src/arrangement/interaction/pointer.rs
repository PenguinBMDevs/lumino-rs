//! Pointer 工具：框选 + 移动已有选择

use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas;

use lumino_core::NotePrecision;

use crate::Message;
use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::auto_scroll::auto_scroll_on_drag;
use crate::arrangement::interaction::geometry::{
    arrange_snapped_bounds, clamped_local, local_pos, snap_tick,
};
use crate::arrangement::interaction::{ArrangementInteractionState, InteractionOutput};

/// Pointer 工具事件入口。
#[allow(clippy::too_many_arguments)]
pub fn handle_pointer_event(
    state: &mut ArrangementInteractionState,
    event: &canvas::Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: &mut ArrangementViewport,
    track_count: usize,
    arr_sel_rect: Option<(f64, f64, usize, usize)>,
    selected_notes: &[(f64, f64, usize, u8)],
    ppq: u16,
    precision: NotePrecision,
    ctrl_pressed: bool,
    _shift_pressed: bool,
) -> InteractionOutput {
    let mut output = InteractionOutput::new();

    // ctrl_pressed 已废弃：Ctrl 多选功能已移除，保留参数以兼容调用签名。
    let _ = ctrl_pressed;

    // 状态清理：若丢失释放事件（如窗口失焦），当检测到主键未按下时重置拖拽。
    if !state.primary_down {
        if state.drag.is_some() {
            state.drag = None;
            output.push(Message::ArrangementDragSelectionRect(None));
        }
        if state.move_drag.is_some() {
            state.move_drag = None;
            state.move_orig_sel = None;
            output.push(Message::ArrangementGhostNotesUpdated(Vec::new()));
            output.push(Message::ArrangementDragSelectionRect(None));
        }
    }

    match event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            state.primary_down = true;
            if let Some(pos) = cursor.position() {
                let local = local_pos(pos, bounds);
                if !bounds.contains(pos) {
                    return output;
                }

                let click_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let click_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();

                if state.hover_inside_selection {
                    // 在已有选择内开始移动
                    state.move_orig_sel = arr_sel_rect;
                    let origin = (click_tick, click_track_f);
                    state.move_drag = Some((origin, origin));
                    state.drag = None;
                    output.push(Message::ArrangementGhostNotesUpdated(Vec::new()));
                } else {
                    // 开始新的框选
                    let start_track_y = (local.y + viewport.scroll_y) / viewport.lane_height();
                    state.drag = Some(((click_tick, start_track_y), local));
                    output.push(Message::ArrangementSelectionCleared);
                }
            }
        }
        canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            // 更新 move_drag 当前位置并生成 ghost 预览
            if let Some((origin, _)) = state.move_drag
                && let Some(pos) = cursor.position()
            {
                let local = local_pos(pos, bounds);
                let current_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let current_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();
                state.move_drag = Some((origin, (current_tick, current_track_f)));
                let ghosts = compute_ghost_notes(state, selected_notes, ppq, precision);
                output.push(Message::ArrangementGhostNotesUpdated(ghosts));

                // 计算移动拖拽中的偏移选择矩形（GPU 渲染用）
                let snapped_origin = snap_tick(origin.0, precision, ppq);
                let snapped_current = snap_tick(current_tick, precision, ppq);
                let dt = (snapped_current - snapped_origin).round() as i64;
                let dtr = (current_track_f - origin.1).round() as i32;
                if let Some((t_start, t_end, track_lo, track_hi)) = state.move_orig_sel {
                    let new_lo = (track_lo as i32 + dtr).max(0) as usize;
                    let new_hi = (track_hi as i32 + dtr).max(0) as usize;
                    if dt != 0 || dtr != 0 {
                        output.push(Message::ArrangementDragSelectionRect(Some((
                            t_start + dt as f64,
                            t_end + dt as f64,
                            new_lo,
                            new_hi,
                        ))));
                    } else {
                        output.push(Message::ArrangementDragSelectionRect(None));
                    }
                }

                // 边缘自动滚动
                auto_scroll_on_drag(
                    pos,
                    bounds,
                    viewport,
                    track_count,
                    &mut state.last_auto_scroll_time,
                    &mut output,
                );
            }
            // 更新 marquee 当前位置
            if let Some((start_music, _)) = state.drag
                && let Some(pos) = cursor.position()
            {
                let local = clamped_local(pos, bounds);
                state.drag = Some((start_music, local));

                // 计算拖拽中的框选矩形（GPU 渲染用）
                let start_pixel = Point::new(
                    viewport.tick_to_x(start_music.0) - viewport.scroll_x,
                    start_music.1 * viewport.lane_height() - viewport.scroll_y,
                );
                let drag_dist = {
                    let v = local - start_pixel;
                    (v.x * v.x + v.y * v.y).sqrt()
                };
                if drag_dist >= 3.0 {
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) =
                        arrange_snapped_bounds(start_pixel, local, viewport, precision, ppq);
                    output.push(Message::ArrangementDragSelectionRect(Some((
                        t_start, t_end, track_lo, track_hi,
                    ))));
                } else {
                    output.push(Message::ArrangementDragSelectionRect(None));
                }

                // 框选时同样支持边缘自动滚动
                auto_scroll_on_drag(
                    pos,
                    bounds,
                    viewport,
                    track_count,
                    &mut state.last_auto_scroll_time,
                    &mut output,
                );
            }
        }
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            state.primary_down = false;
            // 移动释放
            if let Some(((origin_t, origin_tr), (current_t, current_tr))) = state.move_drag.take() {
                state.move_drag = None;
                output.push(Message::ArrangementGhostNotesUpdated(Vec::new()));
                output.push(Message::ArrangementDragSelectionRect(None));
                let snapped_origin = snap_tick(origin_t, precision, ppq);
                let snapped_current = snap_tick(current_t, precision, ppq);
                let delta_ticks = (snapped_current - snapped_origin).round() as i64;
                let delta_tracks = (current_tr - origin_tr).round() as i32;

                if delta_ticks != 0 || delta_tracks != 0 {
                    // 选择在拖拽期间未被清空，arrange_move_notes 可直接找到音符并偏移。
                    // ArrangementMoveNotes handler 会自动 offset 选择矩形到新位置。
                    output.push(Message::ArrangementMoveNotes {
                        delta_ticks,
                        delta_tracks,
                    });
                }
                state.move_orig_sel = None;
                return output;
            }

            // 框选释放
            if let Some((start_music, end_local)) = state.drag.take() {
                state.drag = None;
                output.push(Message::ArrangementDragSelectionRect(None));
                let start_pixel = Point::new(
                    viewport.tick_to_x(start_music.0) - viewport.scroll_x,
                    start_music.1 * viewport.lane_height() - viewport.scroll_y,
                );
                let drag_dist = {
                    let v = end_local - start_pixel;
                    (v.x * v.x + v.y * v.y).sqrt()
                };

                if drag_dist < 3.0 {
                    // 点击：设置光标、清空选择并选中对应音轨
                    let tick = viewport.x_to_tick(start_pixel.x + viewport.scroll_x);
                    let snapped = snap_tick(tick, precision, ppq).max(0.0);
                    output.push(Message::ArrangementSelectionCleared);
                    output.push(Message::ArrangementCursorSet(snapped));

                    let track_idx = start_music.1.floor() as usize;
                    if track_idx < track_count {
                        output.push(lumino_ui_core::sidebar_event::Event::track_selected(
                            track_idx,
                        ));
                    }
                } else {
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) =
                        arrange_snapped_bounds(start_pixel, end_local, viewport, precision, ppq);
                    output.push(Message::ArrangementSelectionChanged(Some((
                        t_start, t_end, track_lo, track_hi,
                    ))));
                }
                return output;
            }
        }
        _ => {}
    }

    output
}

/// 根据当前 move_drag 偏移生成 ghost 音符列表。
fn compute_ghost_notes(
    state: &ArrangementInteractionState,
    selected_notes: &[(f64, f64, usize, u8)],
    ppq: u16,
    precision: NotePrecision,
) -> Vec<(f64, f64, usize)> {
    let mut ghosts = Vec::new();

    let Some(((origin_t, origin_tr), (current_t, current_tr))) = state.move_drag else {
        return ghosts;
    };
    let Some((t_start, t_end, track_lo, track_hi)) = state.move_orig_sel else {
        return ghosts;
    };

    let snapped_origin = snap_tick(origin_t, precision, ppq);
    let snapped_current = snap_tick(current_t, precision, ppq);
    let dt = (snapped_current - snapped_origin).round() as i64;
    let dtr = (current_tr - origin_tr).round() as i32;

    if dt == 0 && dtr == 0 {
        return ghosts;
    }

    let max_track = (track_hi as i32 + dtr).max(0) as usize;

    for (note_start, note_end, track, _key) in selected_notes {
        let track_i32 = *track as i32;
        if track_i32 < track_lo as i32 || track_i32 > track_hi as i32 {
            continue;
        }
        if *note_start < t_start || *note_start > t_end {
            continue;
        }
        let new_start = (*note_start as i64 + dt).max(0) as f64;
        let new_end = (*note_end as i64 + dt).max(new_start as i64) as f64;
        let new_track = (track_i32 + dtr).max(0).min(max_track as i32) as usize;
        ghosts.push((new_start, new_end, new_track));
    }

    ghosts
}
