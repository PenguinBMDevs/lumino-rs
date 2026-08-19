//! Eraser 工具：拖拽矩形擦除音符

use iced_core::{Point, mouse};
use iced_widget::canvas;

use crate::Message;
use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::auto_scroll::auto_scroll_on_drag;
use crate::arrangement::interaction::geometry::{arrange_snapped_bounds, clamped_local, local_pos};
use crate::arrangement::interaction::{
    ArrangementInteractionContext, ArrangementInteractionState, InteractionOutput,
};

/// Eraser 工具事件入口。
pub fn handle_eraser_event(
    state: &mut ArrangementInteractionState,
    viewport: &mut ArrangementViewport,
    ctx: &ArrangementInteractionContext<'_>,
) -> InteractionOutput {
    let mut output = InteractionOutput::new();

    match ctx.event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            state.primary_down = true;
            if let Some(pos) = ctx.cursor.position() {
                if !ctx.bounds.contains(pos) {
                    return output;
                }
                let local = local_pos(pos, ctx.bounds);
                let start_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let start_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();
                state.eraser_drag = Some(((start_tick, start_track_f), local));
            }
        }
        canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            if let Some((start_music, _)) = state.eraser_drag
                && let Some(pos) = ctx.cursor.position()
            {
                let local = clamped_local(pos, ctx.bounds);
                state.eraser_drag = Some((start_music, local));

                // 计算拖拽中的框选矩形（GPU 渲染用）
                let start_pixel = Point::new(
                    viewport.tick_to_x(start_music.0) - viewport.scroll_x,
                    start_music.1 * viewport.lane_height() - viewport.scroll_y,
                );
                let dist = {
                    let v = local - start_pixel;
                    (v.x * v.x + v.y * v.y).sqrt()
                };
                if dist >= 3.0 {
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
            output.push(Message::ArrangementDragSelectionRect(None));
            if let Some((start_music, end_local)) = state.eraser_drag.take() {
                let start_pixel = Point::new(
                    viewport.tick_to_x(start_music.0) - viewport.scroll_x,
                    start_music.1 * viewport.lane_height() - viewport.scroll_y,
                );
                let dist = {
                    let v = end_local - start_pixel;
                    (v.x * v.x + v.y * v.y).sqrt()
                };
                if dist >= 3.0 {
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) = arrange_snapped_bounds(
                        start_pixel,
                        end_local,
                        viewport,
                        ctx.precision,
                        ctx.ppq,
                        ctx.time_signatures,
                    );
                    output.push(Message::ArrangementErase {
                        tick_start: t_start,
                        tick_end: t_end,
                        track_lo,
                        track_hi,
                    });
                }
            }
        }
        _ => {}
    }

    output
}
