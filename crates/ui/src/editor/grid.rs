//! 钢琴卷帘网格绘制模块
//!
//! 该模块已拆分为以下子模块：
//! - `state`: Canvas状态管理
//! - `theme`: 主题颜色工具
//! - `keyboard`: 钢琴键盘绘制
//! - `keys`: 琴键分隔线绘制
//! - `ruler`: 时间轴标尺绘制
//! - `bars`: 小节线/网格线绘制
//! - `remote_cursors`: 远程光标渲染
//! - `selection_box`: 选择框渲染

use crate::constants::editor as editor_constants;
use crate::editor::{Editor, HitType};
use crate::{Message, Renderer, Theme, message::EditorAction};
use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{self, Event, Geometry, Program};

pub mod bars;
pub mod keyboard;
pub mod keys;
pub mod playback_indicator;
pub mod remote_cursors;
pub mod ruler;
pub mod selection_box;
pub mod state;
pub mod theme;

pub use state::CanvasState;

/// 钢琴卷帘网格绘制程序
pub struct PianoRollGrid<'a> {
    pub editor: &'a Editor,
}

impl<'a> PianoRollGrid<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }

    /// 检测双击事件
    fn detect_double_click(&self, state: &mut CanvasState, local_pos: iced_core::Point) -> bool {
        use editor_constants::*;

        let now = std::time::Instant::now();
        let is_double_click = state.last_click_pos.is_some_and(|last_pos| {
            let time_delta = now.duration_since(state.last_click_time).as_millis();
            let pos_delta =
                ((local_pos.x - last_pos.x).powi(2) + (local_pos.y - last_pos.y).powi(2)).sqrt();
            time_delta < DOUBLE_CLICK_TIME_MS && pos_delta < DOUBLE_CLICK_DISTANCE_PX
        });

        if !is_double_click {
            state.last_click_time = now;
            state.last_click_pos = Some(local_pos);
        }

        is_double_click
    }

    /// 处理鼠标左键按下事件
    fn handle_left_press(
        &self,
        state: &mut CanvasState,
        local_pos: iced_core::Point,
    ) -> Option<canvas::Action<Message>> {
        // 检测标尺区域点击（时间轴 scrubbing）
        if local_pos.y < self.editor.state.ruler_height
            && local_pos.x >= self.editor.state.keyboard_width
        {
            // 标尺点击：设置播放位置（scrubbing）
            let tick = self.editor.x_to_tick(local_pos.x);
            let snapped_tick = self.editor.snap_tick(tick).max(0.0);
            return Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Scrubbed { tick: snapped_tick },
            )));
        }

        if self.detect_double_click(state, local_pos) {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::DoubleClicked(local_pos),
            )))
        } else {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Pressed {
                    pos: local_pos,
                    shift: state.shift_pressed,
                },
            )))
        }
    }

    /// 处理滚轮滚动事件
    fn handle_wheel_scroll(&self, delta: &mouse::ScrollDelta) -> Option<canvas::Action<Message>> {
        use editor_constants::*;

        let (delta_x, delta_y) = match delta {
            mouse::ScrollDelta::Lines { x, y } => {
                (*x * SCROLL_LINES_SCALE, *y * SCROLL_LINES_SCALE)
            }
            mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
        };

        let delta_x = delta_x.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);

        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled { delta_x, delta_y },
        )))
    }
}

/// 实现绘制程序接口 - 只绘制网格，不绘制音符
impl<'a> Program<Message, Theme, Renderer> for PianoRollGrid<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // 报告 Canvas 位置和尺寸（用于坐标转换和边界检测）
        let bounds_pos = iced_core::Point::new(bounds.x, bounds.y);
        let bounds_size = iced_core::Size::new(bounds.width, bounds.height);

        let new_size = iced_core::Point::new(bounds.width, bounds.height);
        if self.editor.canvas_size != new_size || self.editor.canvas_offset != bounds_pos {
            return Some(canvas::Action::publish(
                crate::Message::CanvasBoundsChanged {
                    offset: bounds_pos,
                    size: bounds_size,
                },
            ));
        }

        // 同时更新内部状态（鼠标位置）
        if let Some(position) = cursor.position() {
            let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
            state.position = Some(local_pos);
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position() {
                    let local_pos =
                        iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                    return self.handle_left_press(state, local_pos);
                }
            }
            Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift_pressed = modifiers.shift();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                return Some(canvas::Action::publish(Message::EditorAction(
                    EditorAction::Moved(local_pos),
                )));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                return Some(canvas::Action::publish(Message::EditorAction(
                    EditorAction::Released,
                )));
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                return self.handle_wheel_scroll(delta);
            }
            _ => {}
        }

        None
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        use crate::editor::EditState;
        use crate::toolbar::Tool;

        // 橡皮擦工具的光标样式
        if self.editor.current_tool() == Tool::Eraser {
            // 橡皮擦始终显示十字准星，支持点击删除和框选删除
            return mouse::Interaction::Crosshair;
        }

        match self.editor.edit_state {
            EditState::Dragging { .. } => mouse::Interaction::Grabbing,
            EditState::PendingDrag { .. } => mouse::Interaction::Pointer,
            EditState::ResizingStart { .. } | EditState::ResizingEnd { .. } => {
                mouse::Interaction::ResizingHorizontally
            }
            EditState::Drawing { .. } => mouse::Interaction::Crosshair,
            EditState::Selecting { .. } => mouse::Interaction::Crosshair,
            EditState::Scrubbing => mouse::Interaction::Grabbing,
            EditState::Idle => match self.editor.hover_state {
                Some((_, HitType::Start)) | Some((_, HitType::End)) => {
                    mouse::Interaction::ResizingHorizontally
                }
                Some((_, HitType::Middle)) => mouse::Interaction::Pointer,
                None => mouse::Interaction::default(),
            },
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // 网格线现在由 wgpu 直接渲染（见 Host::render_notes）
        // Canvas 只绘制键盘、标尺和交互元素
        let mut geometries = Vec::new();

        // 绘制键盘（使用独立缓存）
        let keyboard_geom = self
            .editor
            .keyboard_cache
            .draw(renderer, bounds.size(), |frame| {
                keyboard::draw(self.editor, frame, bounds, theme);
            });
        geometries.push(keyboard_geom);

        // 绘制标尺（使用独立缓存）
        let ruler_geom = self
            .editor
            .ruler_cache
            .draw(renderer, bounds.size(), |frame| {
                ruler::draw(self.editor, frame, bounds, theme);
            });
        geometries.push(ruler_geom);

        // 绘制框选框（不需要缓存，实时渲染）
        if let Some(selection_geom) = selection_box::draw(self.editor, renderer, theme, bounds) {
            geometries.push(selection_geom);
        }

        // 绘制远端鼠标游标
        let remote_cursor_geometries = remote_cursors::draw(self.editor, renderer, bounds);
        geometries.extend(remote_cursor_geometries);

        // 绘制演奏指示线（实时渲染，不缓存）
        let playback_indicator_geom = playback_indicator::draw(self.editor, renderer, bounds);
        geometries.push(playback_indicator_geom);

        geometries
    }
}

/// 判断琴键是否为黑键（12平均律）
pub fn is_key_dark(key: isize) -> bool {
    let note_in_octave = key % 12;
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 解析十六进制颜色字符串
pub fn parse_color(hex: &str) -> Option<iced_core::Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(iced_core::Color::from_rgb8(r, g, b))
}
