//! Pointer 工具：框选 + 移动已有选择

use iced_core::{Point, mouse};
use iced_widget::canvas;

use lumino_core::NotePrecision;

use crate::Message;
use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::auto_scroll::auto_scroll_on_drag;
use crate::arrangement::interaction::geometry::{
    arrange_snapped_bounds, clamped_local, local_pos, snap_tick,
};
use crate::arrangement::interaction::{
    ArrangementInteractionContext, ArrangementInteractionState, InteractionOutput,
};

/// Pointer 工具事件入口。
pub fn handle_pointer_event(
    state: &mut ArrangementInteractionState,
    viewport: &mut ArrangementViewport,
    ctx: &ArrangementInteractionContext<'_>,
) -> InteractionOutput {
    let mut output = InteractionOutput::new();

    // ctrl_pressed 已废弃：Ctrl 多选功能已移除，保留字段以兼容上下文。
    let _ = ctx.ctrl_pressed;

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

    match ctx.event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            state.primary_down = true;
            if let Some(pos) = ctx.cursor.position() {
                let local = local_pos(pos, ctx.bounds);
                if !ctx.bounds.contains(pos) {
                    return output;
                }

                let click_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let click_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();

                if state.hover_inside_selection {
                    // 在已有选择内开始移动
                    state.move_orig_sel = ctx.arr_sel_rect;
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
                && let Some(pos) = ctx.cursor.position()
            {
                let local = local_pos(pos, ctx.bounds);
                let current_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let current_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();
                state.move_drag = Some((origin, (current_tick, current_track_f)));
                let ghosts = compute_ghost_notes(
                    state,
                    ctx.selected_notes,
                    ctx.ppq,
                    ctx.precision,
                    ctx.time_signatures,
                );
                output.push(Message::ArrangementGhostNotesUpdated(ghosts));

                // 计算移动拖拽中的偏移选择矩形（GPU 渲染用）
                let snapped_origin =
                    snap_tick(origin.0, ctx.precision, ctx.ppq, ctx.time_signatures);
                let snapped_current =
                    snap_tick(current_tick, ctx.precision, ctx.ppq, ctx.time_signatures);
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
                    ctx.bounds,
                    viewport,
                    ctx.track_count,
                    &mut state.last_auto_scroll_time,
                    &mut output,
                );
            }
            // 更新 marquee 当前位置
            if let Some((start_music, _)) = state.drag
                && let Some(pos) = ctx.cursor.position()
            {
                let local = clamped_local(pos, ctx.bounds);
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
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) = arrange_snapped_bounds(
                        start_pixel,
                        local,
                        viewport,
                        ctx.precision,
                        ctx.ppq,
                        ctx.time_signatures,
                    );
                    output.push(Message::ArrangementDragSelectionRect(Some((
                        t_start, t_end, track_lo, track_hi,
                    ))));
                } else {
                    output.push(Message::ArrangementDragSelectionRect(None));
                }

                // 框选时同样支持边缘自动滚动
                auto_scroll_on_drag(
                    pos,
                    ctx.bounds,
                    viewport,
                    ctx.track_count,
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
                let snapped_origin =
                    snap_tick(origin_t, ctx.precision, ctx.ppq, ctx.time_signatures);
                let snapped_current =
                    snap_tick(current_t, ctx.precision, ctx.ppq, ctx.time_signatures);
                let delta_ticks = (snapped_current - snapped_origin).round() as i64;
                let delta_tracks = (current_tr - origin_tr).round() as i32;

                if delta_ticks != 0 || delta_tracks != 0 {
                    // 选择在拖拽期间未被清空，arrange_move_notes 可直接找到音符并偏移。
                    // 移动提交后由 arrange_move_notes 把选择集冻结为本次移动的音符
                    // （精确集合，落点区域内既有音符不进入选择——框选误伤修复）。
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
                    let snapped =
                        snap_tick(tick, ctx.precision, ctx.ppq, ctx.time_signatures).max(0.0);
                    output.push(Message::ArrangementSelectionCleared);
                    output.push(Message::ArrangementCursorSet(snapped));

                    let track_idx = start_music.1.floor() as usize;
                    if track_idx < ctx.track_count {
                        output.push(lumino_ui_core::sidebar_event::Event::track_selected(
                            track_idx,
                        ));
                    }
                } else {
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) = arrange_snapped_bounds(
                        start_pixel,
                        end_local,
                        viewport,
                        ctx.precision,
                        ctx.ppq,
                        ctx.time_signatures,
                    );
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
///
/// 返回 `(start_tick, end_tick, 视觉轨, key)`。**key 必须透传**：覆盖层按
/// `arrangement_note.wgsl` 的同一公式（`lane_top + (127-key)*key_h + key_h/2`）
/// 定位，缺 key 只能退回泳道中线的固定值，ghost 会与真实音符的纵向位置不符。
/// `track` 为**视觉位置**（侧边栏顺序），与 [`crate::arrangement_ops::selection`]
/// 的 `arrangement_selected_notes()` 同源；取色时再由覆盖层映射回文档轨。
#[allow(clippy::too_many_arguments)]
fn compute_ghost_notes(
    state: &ArrangementInteractionState,
    selected_notes: &[(f64, f64, usize, u8)],
    ppq: u16,
    precision: NotePrecision,
    time_signatures: &[(u32, u8, u8)],
) -> Vec<(f64, f64, usize, u8)> {
    let mut ghosts = Vec::new();

    let Some(((origin_t, origin_tr), (current_t, current_tr))) = state.move_drag else {
        return ghosts;
    };
    let Some((t_start, t_end, track_lo, track_hi)) = state.move_orig_sel else {
        return ghosts;
    };

    let snapped_origin = snap_tick(origin_t, precision, ppq, time_signatures);
    let snapped_current = snap_tick(current_t, precision, ppq, time_signatures);
    let dt = (snapped_current - snapped_origin).round() as i64;
    let dtr = (current_tr - origin_tr).round() as i32;

    if dt == 0 && dtr == 0 {
        return ghosts;
    }

    let max_track = (track_hi as i32 + dtr).max(0) as usize;

    for (note_start, note_end, track, key) in selected_notes {
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
        ghosts.push((new_start, new_end, new_track, *key));
    }

    ghosts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造「正在拖动已有选区」的状态。
    ///
    /// `origin` / `current` 为 `(tick, 视觉轨小数)`；`sel` 为移动开始时的
    /// 选择矩形 `(tick_start, tick_end, 视觉轨 lo, 视觉轨 hi)`。
    fn moving_state(
        origin: (f64, f32),
        current: (f64, f32),
        sel: Option<(f64, f64, usize, usize)>,
    ) -> ArrangementInteractionState {
        ArrangementInteractionState {
            move_drag: Some((origin, current)),
            move_orig_sel: sel,
            ..Default::default()
        }
    }

    /// 多 pitch 选区：ghost 必须**逐个保留 key**。
    ///
    /// key 是覆盖层纵向定位的唯一依据（`arrangement_note.wgsl` 的
    /// `note_y = lane_top + (127-key)*key_h + key_h/2`）。旧实现在这里写成 `_key`
    /// 把 key 丢掉，导致预览只能退回泳道中线的固定值——本用例锁死这个回归。
    #[test]
    fn test_ghost_notes_preserve_key_per_pitch() {
        let state = moving_state((0.0, 0.0), (480.0, 1.0), Some((0.0, 960.0, 0, 0)));
        let selected = [(0.0, 480.0, 0, 60u8), (480.0, 960.0, 0, 72u8)];

        let ghosts = compute_ghost_notes(&state, &selected, 480, NotePrecision::Quarter, &[]);

        assert_eq!(
            ghosts,
            vec![(480.0, 960.0, 1, 60), (960.0, 1440.0, 1, 72)],
            "ghost 应带各自的 key 与视觉轨偏移"
        );
    }

    /// 原地按下（无 tick/轨偏移）→ 无 ghost，覆盖层不绘制任何预览。
    #[test]
    fn test_ghost_notes_empty_when_no_offset() {
        let state = moving_state((0.0, 0.0), (0.0, 0.0), Some((0.0, 960.0, 0, 0)));
        let selected = [(0.0, 480.0, 0, 60u8)];

        assert!(
            compute_ghost_notes(&state, &selected, 480, NotePrecision::Quarter, &[]).is_empty(),
            "偏移为零时不得产生 ghost（否则起拖瞬间就出现重影）"
        );
    }

    /// 未处于移动拖拽 / 无原始选择矩形 → 无 ghost。
    #[test]
    fn test_ghost_notes_empty_without_move_drag_or_orig_sel() {
        let selected = [(0.0, 480.0, 0, 60u8)];

        let idle = ArrangementInteractionState::default();
        assert!(
            compute_ghost_notes(&idle, &selected, 480, NotePrecision::Quarter, &[]).is_empty(),
            "非移动拖拽状态不得产生 ghost"
        );

        let no_sel = moving_state((0.0, 0.0), (480.0, 1.0), None);
        assert!(
            compute_ghost_notes(&no_sel, &selected, 480, NotePrecision::Quarter, &[]).is_empty(),
            "缺少原始选择矩形时不得产生 ghost"
        );
    }

    /// 向上越界（dtr 为负）钳制到视觉轨 0，不产生负轨道。
    #[test]
    fn test_ghost_notes_clamp_track_at_zero() {
        let state = moving_state((0.0, 3.0), (0.0, 0.0), Some((0.0, 960.0, 3, 3)));
        let selected = [(0.0, 480.0, 3, 60u8)];

        let ghosts = compute_ghost_notes(&state, &selected, 480, NotePrecision::Quarter, &[]);

        assert_eq!(ghosts, vec![(0.0, 480.0, 0, 60)], "越界上移应钳到视觉轨 0");
    }

    /// 选区外的音符不进入 ghost：视觉轨越界、tick 越界都过滤。
    #[test]
    fn test_ghost_notes_filter_notes_outside_selection() {
        let state = moving_state((0.0, 0.0), (480.0, 1.0), Some((0.0, 960.0, 0, 0)));
        let selected = [
            (0.0, 480.0, 0, 60u8),     // 命中
            (0.0, 480.0, 1, 64u8),     // 视觉轨 1 不在选区
            (2000.0, 2400.0, 0, 67u8), // tick 不在选区
        ];

        let ghosts = compute_ghost_notes(&state, &selected, 480, NotePrecision::Quarter, &[]);

        assert_eq!(
            ghosts,
            vec![(480.0, 960.0, 1, 60)],
            "只有选区内的音符产生 ghost"
        );
    }
}
