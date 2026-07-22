//! Curve 工具：在走带上拖拽绘制音符

use iced_core::{Rectangle, mouse};
use iced_widget::canvas;

use lumino_core::NotePrecision;

use crate::Message;
use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::geometry::{local_pos, snap_tick};
use crate::arrangement::interaction::{ArrangementInteractionState, InteractionOutput};

/// Curve 工具事件入口：拖拽设定音符长度。
///
/// - 左键按下：记录起点（tick, track）。
/// - 拖拽：更新当前局部坐标，供预览绘制。
/// - 左键释放：根据起点与当前点对齐后的 tick 差生成音符。
#[allow(clippy::too_many_arguments)]
pub fn handle_curve_event(
    state: &mut ArrangementInteractionState,
    event: &canvas::Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: &ArrangementViewport,
    track_count: usize,
    ppq: u16,
    precision: NotePrecision,
) -> InteractionOutput {
    let mut output = InteractionOutput::new();

    match event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            if let Some(pos) = cursor.position() {
                if !bounds.contains(pos) {
                    return output;
                }
                let local = local_pos(pos, bounds);
                let tick = viewport.x_to_tick(local.x + viewport.scroll_x);
                let snapped = snap_tick(tick, precision, ppq).max(0.0);
                let track_f = (local.y + viewport.scroll_y) / viewport.lane_height();
                let track = track_f.floor() as usize;
                if track >= track_count {
                    return output;
                }
                state.curve_drag = Some(((snapped, track), local));
            }
        }
        canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            if let Some((start, _)) = state.curve_drag
                && let Some(pos) = cursor.position()
            {
                let local = local_pos(pos, bounds);
                state.curve_drag = Some((start, local));
            }
        }
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            let Some(((start_tick, track), end_local)) = state.curve_drag.take() else {
                return output;
            };
            let end_tick = viewport.x_to_tick(end_local.x + viewport.scroll_x);
            let snapped_end = snap_tick(end_tick, precision, ppq).max(0.0);
            let duration = (snapped_end - start_tick).max(1.0);

            output.push(Message::ArrangementAddNote {
                track,
                tick: start_tick,
                duration,
                key: 60,
                velocity: 100,
            });
        }
        _ => {}
    }

    output
}

/// 计算 Curve 工具当前拖拽预览的音符矩形（tick_start, tick_end, track）。
pub fn curve_preview_note(
    state: &ArrangementInteractionState,
    viewport: &ArrangementViewport,
    ppq: u16,
    precision: NotePrecision,
) -> Option<(f64, f64, usize)> {
    let ((start_tick, track), end_local) = state.curve_drag?;
    let end_tick = viewport.x_to_tick(end_local.x + viewport.scroll_x);
    let snapped_end = snap_tick(end_tick, precision, ppq).max(0.0);
    let t_start = start_tick.min(snapped_end);
    let t_end = start_tick.max(snapped_end).max(t_start + 1.0);
    Some((t_start, t_end, track))
}
