//! 纵向卷帘 Canvas Program — 底部横向钢琴键盘 + 网格线（水平时间 + 垂直音高）
//!
//! 复用横向 `PianoRollGrid` 的交互状态与缩放/滚动语义，仅绘制层转置。

use super::state::GridInteractionState;
use crate::Editor;
use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas::{Action, Event, Geometry, Program};
use lumino_ui_core::constants::editor::{
    PLAYBACK_INDICATOR_TRIANGLE_SIZE, PLAYBACK_INDICATOR_WIDTH,
};
use lumino_ui_core::{Message, Renderer, Theme};

/// 纵向卷帘网格绘制程序
pub struct VerticalRollGrid<'a> {
    pub editor: &'a Editor,
}

impl<'a> VerticalRollGrid<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Program<Message, Theme, Renderer> for VerticalRollGrid<'_> {
    type State = GridInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let bounds_pos = Point::new(bounds.x, bounds.y);
        let bounds_size = iced_core::Size::new(bounds.width, bounds.height);

        let canvas = &self.editor.editor_state.canvas;
        let new_size = Point::new(bounds.width, bounds.height);
        if canvas.size_x != new_size.x
            || canvas.size_y != new_size.y
            || canvas.offset_x != bounds_pos.x
            || canvas.offset_y != bounds_pos.y
        {
            return Some(Action::publish(
                lumino_ui_core::Message::CanvasBoundsChanged {
                    offset: lumino_ui_core::message::Point2::new(bounds_pos.x, bounds_pos.y),
                    size: lumino_ui_core::message::Size2::new(
                        bounds_size.width,
                        bounds_size.height,
                    ),
                },
            ));
        }

        if let Some(position) = cursor.position() {
            let local_pos = Point::new(position.x - bounds.x, position.y - bounds.y);
            state.position = Some(local_pos);
        }

        if let Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.shift_pressed = modifiers.shift();
            state.control_pressed = modifiers.control();
        }

        // 键盘区域滚轮：支持缩放（Ctrl+滚轮）与左右滚动（音高轴 X）
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
            && let Some(pos) = cursor.position_over(bounds)
        {
            let local = Point::new(pos.x - bounds.x, pos.y - bounds.y);
            let view = &self.editor.editor_state.view;
            let keyboard_h = view.keyboard_width;
            let is_over_keyboard = local.y >= bounds.height - keyboard_h;
            if is_over_keyboard {
                let ctrl_pressed = state.control_pressed || self.editor.ctrl_pressed();
                if ctrl_pressed {
                    if let Some(factor) = crate::zoom::zoom_factor_from_delta(delta) {
                        let fixed_ratio = (local.x / bounds.width).clamp(0.0, 1.0);
                        return Some(Action::publish(lumino_ui_core::Message::ZoomYChanged {
                            zoom: view.zoom_y * factor,
                            fixed_ratio,
                        }));
                    }
                } else {
                    // 普通滚轮：垂直增量映射为水平滚动（自然滚动方向与横向键盘一致）
                    let (_, delta_y) = match delta {
                        mouse::ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                        mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                    };
                    // 优先使用垂直增量，兼容触控板水平手势
                    let mut delta_h = delta_y;
                    if delta_h.abs() < f32::EPSILON {
                        if let mouse::ScrollDelta::Lines { x, .. } = delta {
                            delta_h = *x * 20.0;
                        } else if let mouse::ScrollDelta::Pixels { x, .. } = delta {
                            delta_h = *x;
                        }
                    }
                    delta_h = delta_h.clamp(-120.0, 120.0);
                    if delta_h.abs() > f32::EPSILON {
                        return Some(Action::publish(lumino_ui_core::Message::EditorAction(
                            lumino_ui_core::message::EditorAction::Scrolled {
                                delta_x: 0.0,
                                delta_y: delta_h,
                            },
                        )));
                    }
                }
            } else if local.y >= view.ruler_height && local.y < bounds.height - keyboard_h {
                // 网格区域：Y 向时间轴（头部在键盘顶部，向上递增）支持滚动与缩放
                let ctrl_pressed = state.control_pressed || self.editor.ctrl_pressed();
                let grid_top = view.ruler_height;
                let grid_bottom = bounds.height - keyboard_h;
                let grid_h = (grid_bottom - grid_top).max(1.0);
                if ctrl_pressed {
                    if let Some(factor) = crate::zoom::zoom_factor_from_delta(delta) {
                        // 锚点距底部比例：0=键盘顶部，1=顶部标尺
                        let fixed_ratio = ((grid_bottom - local.y) / grid_h).clamp(0.0, 1.0);
                        return Some(Action::publish(lumino_ui_core::Message::ZoomXChanged {
                            zoom: view.zoom_x * factor,
                            fixed_ratio,
                        }));
                    }
                } else {
                    // 普通滚轮：垂直滚动时间轴（Y，头部在底部），水平滚动音高轴（X）
                    let (delta_x, delta_y) = match delta {
                        mouse::ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                        mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                    };
                    let mut out_delta_x = 0.0;
                    let mut out_delta_y = 0.0;
                    // 垂直增量 -> 时间轴（scroll_x），取反使向上滚显示更后时间（与横向一致）
                    if delta_y.abs() > f32::EPSILON {
                        out_delta_x = (-delta_y).clamp(-120.0, 120.0);
                    }
                    // 水平增量 -> 音高轴（scroll_y），取反使向右滚显示更高音
                    if delta_x.abs() > f32::EPSILON {
                        out_delta_y = (-delta_x).clamp(-120.0, 120.0);
                    }
                    // Shift+滚轮：垂直转水平（触控板兼容）
                    if state.shift_pressed
                        && out_delta_x.abs() < f32::EPSILON
                        && delta_y.abs() > f32::EPSILON
                    {
                        out_delta_y = (-delta_y).clamp(-120.0, 120.0);
                        out_delta_x = 0.0;
                    }
                    if out_delta_x.abs() > f32::EPSILON || out_delta_y.abs() > f32::EPSILON {
                        return Some(Action::publish(lumino_ui_core::Message::EditorAction(
                            lumino_ui_core::message::EditorAction::Scrolled {
                                delta_x: out_delta_x,
                                delta_y: out_delta_y,
                            },
                        )));
                    }
                }
            }
        }

        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Idle
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut geometries = Vec::new();

        // 1. 网格线与音符已迁移至 wgpu（infinite_grid_vertical.wgsl + note_vertical.wgsl），
        //    复用加载 MIDI 后同批 NoteInstance GPU 数据，仅转置坐标（瀑布流风格纵向流动），
        //    样式完移植横向 LOD 且 Key 范围八度分割更明显（C 音 2px/0.95 alpha）；
        //    Canvas 层不再绘制网格/音符，避免与 wgpu 离屏纹理重叠，仅保留键盘与指示线。
        //    （保留 vertical_bars.rs 作离线校验与单元测试，不再参与实时绘制）

        // 2. 底部横向钢琴键盘（缓存与横向一致：复用 keyboard_cache 语义，但此处每帧直绘）
        let kb_geom = {
            let mut frame = iced_widget::canvas::Frame::new(renderer, bounds.size());
            super::vertical_keyboard::draw(self.editor, &mut frame, bounds, theme);
            frame.into_geometry()
        };
        geometries.push(kb_geom);

        // 2.2 小节号文本与边框（网格线已由 wgpu 绘制，此处仅保留文本，避免重复）
        let label_geom = {
            let mut frame = iced_widget::canvas::Frame::new(renderer, bounds.size());
            super::vertical_bars::draw_labels(self.editor, &mut frame, bounds, theme);
            frame.into_geometry()
        };
        geometries.push(label_geom);

        // 2.1 洋葱皮覆盖层
        if let Some(geom) =
            super::vertical_keyboard::draw_onion_overlay(self.editor, renderer, bounds)
        {
            geometries.push(geom);
        }

        // 3. 播放指示线（水平红线，时间轴在 Y）
        if let Some(geom) = draw_vertical_playback(self.editor, renderer, bounds) {
            geometries.push(geom);
        }

        geometries
    }
}

fn draw_vertical_playback(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    use iced_widget::canvas::{Frame, Path, Stroke};

    let view = &editor.editor_state.view;
    let keyboard_h = view.keyboard_width;
    if bounds.height <= view.ruler_height + keyboard_h {
        return None;
    }
    let grid_top = view.ruler_height;
    let grid_bottom = bounds.height - keyboard_h;

    // 计算播放指示线 Y（纵向：时间轴在 Y，头部在键盘顶部，向上递增）
    let indicator_y = grid_bottom - editor.playback_position * view.zoom_x + view.scroll_x;

    if indicator_y < grid_top || indicator_y > grid_bottom {
        return None;
    }

    let mut frame = Frame::new(renderer, bounds.size());
    let indicator_color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);
    let line_path = Path::line(
        Point::new(0.0, indicator_y),
        Point::new(bounds.width, indicator_y),
    );
    frame.stroke(
        &line_path,
        Stroke::default()
            .with_width(PLAYBACK_INDICATOR_WIDTH)
            .with_color(indicator_color),
    );
    // 左侧三角形指示
    let tri = PLAYBACK_INDICATOR_TRIANGLE_SIZE;
    let triangle_path = Path::new(|b| {
        let top = Point::new(0.0, indicator_y - tri / 2.0);
        let bottom = Point::new(0.0, indicator_y + tri / 2.0);
        let right = Point::new(tri, indicator_y);
        b.move_to(top);
        b.line_to(bottom);
        b.line_to(right);
        b.close();
    });
    frame.fill(&triangle_path, indicator_color);

    Some(frame.into_geometry())
}
