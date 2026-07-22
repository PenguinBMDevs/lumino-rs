//! Eraser 工具：拖拽矩形擦除音符

use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas;

use lumino_core::NotePrecision;

use crate::Message;
use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::auto_scroll::auto_scroll_on_drag;
use crate::arrangement::interaction::geometry::{arrange_snapped_bounds, clamped_local, local_pos};
use crate::arrangement::interaction::{ArrangementInteractionState, InteractionOutput};

/// Eraser 工具事件入口。
#[allow(clippy::too_many_arguments)]
pub fn handle_eraser_event(
    state: &mut ArrangementInteractionState,
    event: &canvas::Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: &mut ArrangementViewport,
    track_count: usize,
    ppq: u16,
    precision: NotePrecision,
) -> InteractionOutput {
    let mut output = InteractionOutput::new();

    match event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            state.primary_down = true;
            if let Some(pos) = cursor.position() {
                if !bounds.contains(pos) {
                    return output;
                }
                let local = local_pos(pos, bounds);
                let start_tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let start_track_f = (local.y + viewport.scroll_y) / viewport.lane_height();
                state.eraser_drag = Some(((start_tick, start_track_f), local));
            }
        }
        canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            if let Some((start_music, _)) = state.eraser_drag
                && let Some(pos) = cursor.position()
            {
                let local = clamped_local(pos, bounds);
                state.eraser_drag = Some((start_music, local));

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
                    let (_, _, _, _, t_start, t_end, track_lo, track_hi) =
                        arrange_snapped_bounds(start_pixel, end_local, viewport, precision, ppq);
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
