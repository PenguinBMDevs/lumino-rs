use crate::{Message, Renderer, Theme};
use crate::editor::note::Note;
use crate::editor::state::ViewState;
use iced_core::{Point, Rectangle, mouse};
use iced_wgpu::geometry;
use iced_widget::canvas::{self, Frame, Geometry, Path, Program, Stroke, Event};

/// 钢琴卷帘网格绘制程序
pub struct PianoRollGrid<'a> {
    pub state: &'a ViewState,
    pub grid_cache: &'a canvas::Cache<Renderer>,
    pub note_cache: &'a canvas::Cache<Renderer>,
}



/// 实现绘制程序接口
impl<'a> Program<Message, Theme, Renderer> for PianoRollGrid<'a> {
    // State 存储鼠标位置
    type State = Option<Point>;

    // 实时更新位置
    fn update(
        &self,
        state: &mut Self::State,
        _event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let Some(position) = cursor.position() {
            // 将鼠标坐标转换为 Canvas 局部坐标
            let local_pos = Point::new(position.x - bounds.x, position.y - bounds.y);
            *state = Some(local_pos);
            // 清除音符缓存，强制重绘
            self.note_cache.clear();
            Some(canvas::Action::request_redraw())
        } else {
            None
        }
    }

    // 启用鼠标交互
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Crosshair
    }

    // 绘制函数，这里是绘制 PianoRollGrid 的主要逻辑
    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // 1. 绘制缓存的网格（静态内容）
        let grid_geometry = self.grid_cache.draw(renderer, bounds.size(), |frame| {
            self.draw_keys(frame, bounds, theme);
            self.draw_bars(frame, bounds, theme);
        });

        // 2. 绘制音符（动态内容，每次清除缓存后重绘）
        let note_geometry = self.note_cache.draw(renderer, bounds.size(), |frame| {
            if let Some(pos) = *state {
                let note = Note::from_mouse_position(pos, self.state.scroll_x, self.state.scroll_y, theme);
                note.draw(frame);
            }
        });

        vec![grid_geometry, note_geometry]
    }
}



/// 绘制横向线，包括琴键分隔线
impl<'a> PianoRollGrid<'a> {
    /// 绘制横向线，包括琴键分隔线
    fn draw_keys(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        // 视图状态
        let view = self.state;
        // 线条粗细和颜色
        let line_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);
        // 最高琴键索引
        let max_key_index = (view.key_count - 1) as f32;
        // 绘制琴键分隔线
        for i in 0..view.key_count {
            let keynum = i as isize;

            // 坐标计算
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;
            // 绘制底部分割线
            let line_y = screen_y + view.zoom_y;
            let path = Path::line(Point::new(0.0, line_y), Point::new(bounds.width, line_y));
            frame.stroke(&path, line_stroke);
        }
    }

    /// 绘制纵向线，包括小节线和拍线
    fn draw_bars(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let view = self.state;
        let ppq = view.ppq as f32;
        let palette = theme.extended_palette().background;

        // 这里只是随便写个四四拍，这个会根据歌曲变化
        let measure_ticks = ppq * 4.0;

        // 计算当前视图可见范围的tick范围
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + bounds.width) / view.zoom_x;

        // 计算第一个需要绘制的小节开始位置
        let mut current_tick = (start_tick / ppq).ceil() * ppq;

        // 线条样式，跟随主题变化
        let bar_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);
        let beat_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);

        // 绘制小节线和小节内拍线
        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x;
            // 小节线和拍线的判断逻辑
            let is_measure = (current_tick % measure_ticks).abs() < 0.1;
            let stroke = if is_measure { bar_stroke } else { beat_stroke };
            // 绘制小节线和小节内拍线
            let path = Path::line(
                Point::new(screen_x, 0.0),
                Point::new(screen_x, bounds.height),
            );
            frame.stroke(&path, stroke);
            current_tick += ppq;
        }
    }
}

#[allow(dead_code)]
/// 判断某个琴键是否该涂黑，它不是指这里是不是黑键，而是根据你选择什么调式灵活处理
fn is_key_dark(key: isize, _key_count: usize) -> bool {
    // 先这么写，这是12平均律
    let note_in_octave = key % 12;
    match note_in_octave {
        1 | 3 | 6 | 8 | 10 => true,
        _ => false,
    }
}
