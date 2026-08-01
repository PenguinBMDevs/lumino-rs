//! 工程走带视图点击 Canvas —— 透明覆盖层，捕获鼠标事件
//!
//! 工程走带视图的主体由 WGPU ArrangementRenderer 渲染，但 WGPU 层不处理鼠标事件。
//! 此 Canvas 作为透明覆盖层叠加在走带区域上方，按当前工具分发事件：
//! - Pointer：框选、移动已有选择、点击设置光标、点击选中音轨
//! - Eraser：拖拽矩形擦除音符
//! - Curve：点击绘制音符

use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{self, Frame, Geometry, Program};
use lumino_core::{NotePrecision, Tool};

use crate::arrangement::ArrangementViewport;
use crate::arrangement::interaction::curve::curve_preview_note;
use crate::arrangement::interaction::{
    ArrangementInteractionContext, ArrangementInteractionState, handle_event,
};
use crate::{Message, Renderer, Theme};

/// 工程走带点击 Canvas
pub struct ArrangementClickCanvas {
    /// 视口引用（用于坐标转换）
    pub viewport: ArrangementViewport,
    /// 当前激活工具，决定鼠标交互行为
    pub current_tool: Tool,
    /// 当前总轨道数
    pub track_count: usize,
    /// 当前已提交的选择矩形（tick_start, tick_end, track_lo, track_hi）
    pub arr_sel_rect: Option<(f64, f64, usize, usize)>,
    /// 当前选中的音符（tick_start, tick_end, track, key），用于移动时 ghost 预览
    pub selected_notes: Vec<(f64, f64, usize, u8)>,
    /// 每四分音符 tick 数
    pub ppq: u16,
    /// 网格对齐精度
    pub precision: NotePrecision,
    /// Ctrl 键按下状态
    pub ctrl_pressed: bool,
    /// Shift 键按下状态
    pub shift_pressed: bool,
}

impl ArrangementClickCanvas {
    /// 处理中键拖拽平移与滚轮缩放/滚动。
    ///
    /// 返回 `Some(action)` 当事件被导航逻辑消费，此时不再分发给工具事件处理。
    fn handle_navigation_event(
        &self,
        state: &mut ArrangementInteractionState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        track_count: usize,
    ) -> Option<canvas::Action<Message>> {
        use iced_core::mouse;
        use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(pos) = cursor.position()
                    && bounds.contains(pos)
                {
                    state.middle_drag = Some(crate::arrangement::interaction::geometry::local_pos(
                        pos, bounds,
                    ));
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let start = state.middle_drag?;
                let pos = cursor.position()?;
                let current = crate::arrangement::interaction::geometry::local_pos(pos, bounds);
                let delta = current - start;
                if delta.x == 0.0 && delta.y == 0.0 {
                    return None;
                }
                state.middle_drag = Some(current);

                let mut viewport = self.viewport.clone();
                viewport.scroll_x -= delta.x;
                viewport.scroll_y -= delta.y;
                viewport.clamp_scroll(track_count);

                Some(canvas::Action::publish(Message::Batch(vec![
                    Message::ArrangementScrollX(viewport.scroll_x),
                    Message::ArrangementScrollY(viewport.scroll_y),
                ])))
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                state.middle_drag = None;
                None
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let pos = cursor.position()?;
                if !bounds.contains(pos) {
                    return None;
                }
                let local = crate::arrangement::interaction::geometry::local_pos(pos, bounds);

                let (mut dx, mut dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => {
                        (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                    }
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                dx = dx.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);

                if self.ctrl_pressed {
                    // Ctrl + 滚轮：水平缩放
                    let factor = if dy > 0.0 { 1.1 } else { 0.9 };
                    let new_zoom = (self.viewport.zoom_x * factor).clamp(
                        crate::constants::editor::zoom::MIN_ARRANGEMENT_ZOOM_X,
                        crate::constants::editor::zoom::MAX_ARRANGEMENT_ZOOM_X,
                    );
                    let fixed_ratio = if bounds.width > 0.0 {
                        local.x / bounds.width
                    } else {
                        0.0
                    };
                    Some(canvas::Action::publish(Message::ArrangementZoomX {
                        zoom: new_zoom,
                        fixed_ratio,
                    }))
                } else if self.shift_pressed {
                    // Shift + 滚轮：水平滚动
                    Some(canvas::Action::publish(Message::ArrangementScrollX(
                        self.viewport.scroll_x + dx,
                    )))
                } else {
                    // 普通滚轮：垂直滚动
                    Some(canvas::Action::publish(Message::ArrangementScrollY(
                        self.viewport.scroll_y - dy,
                    )))
                }
            }
            _ => None,
        }
    }

    /// 根据当前工具返回合适的鼠标交互形态
    fn interaction_for_tool(&self, state: &ArrangementInteractionState) -> mouse::Interaction {
        match self.current_tool {
            Tool::Pointer => {
                if state.is_dragging() {
                    mouse::Interaction::Crosshair
                } else if state.hover_inside_selection {
                    mouse::Interaction::Grab
                } else {
                    mouse::Interaction::Pointer
                }
            }
            Tool::Curve | Tool::Eraser => mouse::Interaction::Crosshair,
            _ => mouse::Interaction::default(),
        }
    }
}

impl Program<Message, Theme, Renderer> for ArrangementClickCanvas {
    type State = ArrangementInteractionState;

    fn update(
        &self,
        state: &mut ArrangementInteractionState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        puffin::profile_function!();

        // 中键拖拽平移与滚轮缩放/滚动优先于工具事件
        if let Some(action) =
            self.handle_navigation_event(state, event, bounds, cursor, self.track_count)
        {
            return Some(action);
        }

        // viewport 可变副本：交互可能修改滚动（自动滚动预览）
        let mut viewport = self.viewport.clone();
        let ctx = ArrangementInteractionContext {
            event,
            bounds,
            cursor,
            current_tool: self.current_tool,
            track_count: self.track_count,
            arr_sel_rect: self.arr_sel_rect,
            selected_notes: &self.selected_notes,
            ppq: self.ppq,
            precision: self.precision,
            ctrl_pressed: self.ctrl_pressed,
            shift_pressed: self.shift_pressed,
        };
        let messages = handle_event(state, &mut viewport, &ctx);

        match messages.len() {
            0 => None,
            1 => messages.into_iter().next().map(canvas::Action::publish),
            _ => Some(canvas::Action::publish(Message::Batch(messages))),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_function!();

        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let ghost_color = palette.background.strong.text;

        // 绘制框选/移动/橡皮擦预览矩形（全部由 GPU 渲染）
        // 保留 Empty 路径以消费 state，避免编译器警告
        match self.current_tool {
            Tool::Pointer => {
                // 状态已由 handle_pointer_event 通过消息同步到 GPU
                let _ = state;
            }
            Tool::Curve => {
                // Curve 拖拽时绘制音符长度预览（仍在 CPU Canvas，轻量反馈）
                if let Some((t_start, t_end, track)) =
                    curve_preview_note(state, &self.viewport, self.ppq, self.precision)
                {
                    draw_ghost_note(
                        &mut frame,
                        &self.viewport,
                        t_start,
                        t_end,
                        track,
                        ghost_color,
                    );
                }
            }
            Tool::Eraser => {
                // 状态已由 handle_eraser_event 通过消息同步到 GPU
                let _ = state;
            }
            _ => {}
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        self.interaction_for_tool(state)
    }
}

fn draw_ghost_note(
    frame: &mut Frame<Renderer>,
    viewport: &ArrangementViewport,
    t_start: f64,
    t_end: f64,
    track: usize,
    color: iced_core::Color,
) {
    let lh = viewport.lane_height();
    let scroll_y = viewport.scroll_y;
    let click_y = track as f32 * lh - scroll_y;
    let scroll_x = viewport.scroll_x;
    let sx = viewport.tick_to_x(t_start) - scroll_x;
    let ex = viewport.tick_to_x(t_end) - scroll_x;
    let min_x = sx.min(ex);
    let max_x = sx.max(ex);

    let height = lh * 0.25;
    let y_center = click_y + lh * 0.5;

    let rect = iced_core::Rectangle {
        x: min_x,
        y: y_center - height * 0.5,
        width: max_x - min_x,
        height,
    };

    let fill = color.scale_alpha(0.4);
    frame.fill_rectangle(rect.position(), rect.size(), fill);
}
