use crate::{Message, Renderer, Theme};
use crate::editor::state::ViewState;
use iced_core::{Point, Rectangle, mouse};

use iced_widget::canvas::{self, Frame, Geometry, Path, Program, Stroke, Event};

/// 钢琴卷帘网格绘制程序
pub struct PianoRollGrid<'a> {
    pub state: &'a ViewState,
    pub grid_cache: &'a canvas::Cache<Renderer>,
}

impl<'a> PianoRollGrid<'a> {
    pub fn new(state: &'a ViewState, grid_cache: &'a canvas::Cache<Renderer>) -> Self {
        Self { state, grid_cache }
    }
}

/// 存储 Canvas 状态的类型
pub type CanvasState = Option<iced_core::Point>;

/// 实现绘制程序接口 - 只绘制网格，不绘制音符
impl<'a> Program<Message, Theme, Renderer> for PianoRollGrid<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        _event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // 报告 Canvas 位置和尺寸（用于坐标转换和边界检测）
        let bounds_pos = iced_core::Point::new(bounds.x, bounds.y);
        let bounds_size = iced_core::Size::new(bounds.width, bounds.height);
        
        // 同时更新内部状态（鼠标位置）
        if let Some(position) = cursor.position() {
            let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
            *state = Some(local_pos);
        }
        
        // 发送 bounds 变化消息
        Some(canvas::Action::publish(Message::CanvasBoundsChanged { 
            offset: bounds_pos, 
            size: bounds_size 
        }))
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Crosshair
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        self.draw_keyboard(&mut frame, bounds, theme);
        self.draw_keys(&mut frame, bounds, theme);
        self.draw_bars(&mut frame, bounds, theme);
        
        vec![frame.into_geometry()]
    }
}

/// 网格绘制实现
impl<'a> PianoRollGrid<'a> {
    /// 绘制钢琴键盘（左侧键位指示器）
    fn draw_keyboard(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        let view = self.state;
        let keyboard_width = view.keyboard_width;
        let max_key_index = (view.visible_key_count - 1) as f32;

        for i in 0..view.visible_key_count {
            let keynum = i as isize;
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;

            if screen_y + view.zoom_y >= 0.0 && screen_y <= bounds.height {
                let is_black_key = is_key_dark(keynum, view.visible_key_count as usize);
                let key_color = if is_black_key {
                    palette.stronger.color
                } else {
                    palette.base.color
                };

                let key_rect = Rectangle::new(
                    Point::new(0.0, screen_y),
                    iced_core::Size::new(keyboard_width, view.zoom_y),
                );
                let key_path = Path::rectangle(key_rect.position(), key_rect.size());
                frame.fill(&key_path, key_color);

                let border_stroke = Stroke::default()
                    .with_width(1.0)
                    .with_color(palette.strong.color);
                frame.stroke(&key_path, border_stroke);
            }
        }
    }

    /// 绘制琴键分隔线（横向线）
    fn draw_keys(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        let view = self.state;
        let line_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);

        let keyboard_width = view.keyboard_width;
        let max_key_index = (view.visible_key_count - 1) as f32;

        for i in 0..view.visible_key_count {
            let keynum = i as isize;
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;

            if screen_y + view.zoom_y >= 0.0 && screen_y <= bounds.height {
                let line_y = screen_y + view.zoom_y;
                let path = Path::line(
                    Point::new(keyboard_width, line_y),
                    Point::new(bounds.width, line_y),
                );
                frame.stroke(&path, line_stroke);
            }
        }
    }

    /// 绘制小节线和拍线（纵向线）
    fn draw_bars(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let view = self.state;
        let ppq = view.ppq as f32;
        let palette = theme.extended_palette().background;
        let keyboard_width = view.keyboard_width;

        let measure_ticks = ppq * 4.0;
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

        // 网格线间隔：ppq/4 = 480 ticks
        let grid_gap = ppq / 4.0;
        let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

        let bar_stroke = Stroke::default()
            .with_width(2.0)
            .with_color(palette.strong.color);
        let beat_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.weak.color);
        let sub_beat_stroke = Stroke::default()
            .with_width(0.5)
            .with_color(palette.weak.color);

        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x + keyboard_width;
            
            if screen_x >= keyboard_width && screen_x <= bounds.width {
                let is_measure = (current_tick % measure_ticks).abs() < 0.1;
                let is_beat = (current_tick % ppq).abs() < 0.1;
                let is_half_beat = (current_tick % (ppq/2.0)).abs() < 0.1;
                
                let stroke = if is_measure { 
                    bar_stroke 
                } else if is_beat {
                    beat_stroke
                } else if is_half_beat {
                    sub_beat_stroke
                } else {
                    Stroke::default()
                        .with_width(0.5)
                        .with_color(iced_core::Color{a: 0.1, ..palette.weak.color})
                };

                let path = Path::line(
                    Point::new(screen_x, 0.0),
                    Point::new(screen_x, bounds.height),
                );
                frame.stroke(&path, stroke);
            }
            current_tick += grid_gap;
        }
    }
}

/// 判断琴键是否为黑键（12平均律）
fn is_key_dark(key: isize, _key_count: usize) -> bool {
    let note_in_octave = key % 12;
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}
