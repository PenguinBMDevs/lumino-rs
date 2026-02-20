use crate::{Message, Renderer, Theme};
use crate::editor::note::Note;
use crate::editor::state::ViewState;
use iced_core::{Point, Rectangle, mouse};

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
        // tracing::info!("Drawing grid with scroll_x: {}", self.state.scroll_x); // debug log
        // 禁用缓存，直接绘制（测试用）
        let mut frame = Frame::new(renderer, bounds.size());
        self.draw_keyboard(&mut frame, bounds, theme);
        self.draw_keys(&mut frame, bounds, theme);
        self.draw_bars(&mut frame, bounds, theme);
        let grid_geometry = frame.into_geometry();

        // 音符同样直接绘制
        let mut note_frame = Frame::new(renderer, bounds.size());
        if let Some(pos) = *state {
            let note = Note::from_mouse_position(pos, self.state, theme);
            note.draw(&mut note_frame);
        }
        let note_geometry = note_frame.into_geometry();

        vec![grid_geometry, note_geometry]
    }
}



/// 绘制横向线，包括琴键分隔线
impl<'a> PianoRollGrid<'a> {
    /// 绘制钢琴键盘
    fn draw_keyboard(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        let view = self.state;

        // 键盘宽度（从状态获取）
        let keyboard_width = view.keyboard_width;

        // 最高琴键索引
        let max_key_index = (view.visible_key_count - 1) as f32;

        // 绘制琴键
        for i in 0..view.visible_key_count {
            let keynum = i as isize;

            // 坐标计算（与分割线对齐）
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;

            // 只绘制在屏幕可见范围内的键
            if screen_y + view.zoom_y >= 0.0 && screen_y <= bounds.height {
                let key_height = view.zoom_y;
                let key_y = screen_y;

                // 判断是否为黑键
                let is_black_key = is_key_dark(keynum, view.visible_key_count as usize);

                // 键的颜色
                let key_color = if is_black_key {
                    palette.stronger.color // 黑键
                } else {
                    palette.base.color   // 白键
                };

                // 绘制键的矩形
                let key_rect = Rectangle::new(
                    Point::new(0.0, key_y),
                    iced_core::Size::new(keyboard_width, key_height),
                );
                let key_path = Path::rectangle(key_rect.position(), key_rect.size());
                frame.fill(&key_path, key_color);

                // 为键添加边框
                let border_stroke = Stroke::default()
                    .with_width(1.0)
                    .with_color(palette.strong.color);
                frame.stroke(&key_path, border_stroke);
            }
        }
    }

    /// 绘制横向线，包括琴键分隔线
    fn draw_keys(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        // 视图状态
        let view = self.state;
        // 线条粗细和颜色
        let line_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);

        // 键盘宽度
        let keyboard_width = view.keyboard_width;

        // 最高琴键索引
        let max_key_index = (view.visible_key_count - 1) as f32;
        // 绘制琴键分隔线
        for i in 0..view.visible_key_count {
            let keynum = i as isize;

            // 坐标计算
            // 注意：这里我们让 keynum = 0 (最低音) 在最下面，keynum = max_key_index (最高音) 在最上面
            // 当 scroll_y = 0 时，最高音在屏幕顶部 (y=0)
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;

            // 只绘制在屏幕可见范围内的线
            if screen_y + view.zoom_y >= 0.0 && screen_y <= bounds.height {
                // 绘制底部分割线（从键盘右侧开始）
                let line_y = screen_y + view.zoom_y;
                let path = Path::line(Point::new(keyboard_width, line_y), Point::new(bounds.width, line_y));
                frame.stroke(&path, line_stroke);
            }
        }
    }

    /// 绘制纵向线，包括小节线和拍线
    fn draw_bars(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let view = self.state;
        let ppq = view.ppq as f32;
        let palette = theme.extended_palette().background;

        // 键盘宽度
        let keyboard_width = view.keyboard_width;

        // 这里只是随便写个四四拍，这个会根据歌曲变化
        let measure_ticks = ppq * 4.0;

        // 计算当前视图可见范围的tick范围（只计算键盘右侧的区域）
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

        // 计算第一个需要绘制的小节开始位置
        // 修正：我们要绘制所有拍子，不仅仅是小节线，还要考虑 snap_precision
        // 这里暂时保持以 ppq 为单位绘制拍子线，或者根据 snap_precision 绘制更细的网格
        // 为了和音符对齐一致，建议至少能看到拍子线
        // 我们需要和 default_note_length 对齐，default_note_length 是 480，即 ppq/4
        // 所以网格线间隔应该是 ppq/4 = 480 ticks
        let grid_gap = ppq / 4.0; // 480 ticks
        let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

        // 线条样式，跟随主题变化
        let bar_stroke = Stroke::default()
            .with_width(2.0)
            .with_color(palette.strong.color);
        let beat_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.weak.color);
        let sub_beat_stroke = Stroke::default()
            .with_width(0.5) // 更细的线
            .with_color(palette.weak.color);

        // 绘制小节线和小节内拍线
        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x + keyboard_width;
            
            // 只绘制在键盘右侧的线条
            if screen_x >= keyboard_width && screen_x <= bounds.width {
                // 小节线和拍线的判断逻辑
                // 小节线：每 4 拍
                let is_measure = (current_tick % measure_ticks).abs() < 0.1;
                // 拍子线：每 1 拍 (1920 ticks)
                let is_beat = (current_tick % ppq).abs() < 0.1;
                // 半拍子线：每 1/2 拍 (960 ticks)
                let is_half_beat = (current_tick % (ppq/2.0)).abs() < 0.1;
                
                let stroke = if is_measure { 
                    bar_stroke 
                } else if is_beat {
                    beat_stroke
                } else if is_half_beat {
                    sub_beat_stroke
                } else {
                    // 1/4 拍子线
                     Stroke::default().with_width(0.5).with_color(iced_core::Color{a: 0.1, ..palette.weak.color})
                };

                // 绘制网格线
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

#[allow(dead_code)]
/// 判断某个琴键是否该涂黑，它不是指这里是不是黑键，而是根据你选择什么调式灵活处理
fn is_key_dark(key: isize, _key_count: usize) -> bool {
    // 先这么写，这是12平均律
    let note_in_octave = key % 12;
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}
