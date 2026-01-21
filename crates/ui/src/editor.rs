use iced_aw::core::renderer;
use iced_core::{Length, Rectangle, Point, Size, Vector, mouse, Color};
use iced_widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Program};
use lumino_core::event::menu::view;
use crate::{Message, Theme, Renderer};
use super::Element;

#[derive(Debug, Clone)]
pub struct ViewState {
    pub scroll_x: f32,  // x轴滚动位置，对应歌曲位置，单位为tick
    pub scroll_y: f32,  // y轴滚动位置，对应键盘位置，单位可能为pixel

    pub zoom_x: f32,    // 横向缩放: Pixels per Tick
    pub zoom_y: f32,    // 纵向缩放: Pixels per Key

    pub key_count: u16, // 键盘总键数，默认128，目前计划支持88/128/256键
    pub ppq: u16,       // 分辨率，整数，默认设定为1920，最大值65535
    //pub scale: Scale  // TODO: 之后我们需要支持不同的调式/微分音
}

impl Default for ViewState {
    fn default() -> Self {      // 这里给个默认值，默认打开钢琴卷帘就是这样的坐标位置和大小
        Self {
            scroll_x: 0.0,      // 歌曲位置0tick
            scroll_y: 0.0,      // 理应把焦点放在中间音区最合适，之后看看多少像素最合适
            zoom_x: 0.1,        // 每像素10tick，gate1920的音符长度是1920像素
            zoom_y: 20.0,       // 琴键高度20像素
            key_count: 128,     // 显示为128键（不影响MIDI内部数据）
            ppq: 1920,          // 分辨率1920
        }
    }
}
pub struct Editor {
    state: ViewState,
    grid_cache: canvas::Cache<Renderer>,  // 介绍一下，没有这个cache，60帧每帧都要重绘，卡死你
}

impl Editor {
    pub fn new() -> Self {
        Self {
            state: ViewState::default(),    // 使用默认值坐标、缩放
            grid_cache: canvas::Cache::new(),
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a> {
        //container(space()).width(Length::Fill).into() //原来写的
        Canvas::new(PianoRollGrid {
            state: &self.state,
            cache: &self.grid_cache,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()                     // 这里大小就是Canvas控件大小
    }
}

struct PianoRollGrid<'a> {
    state: &'a ViewState,
    cache: &'a canvas::Cache<Renderer>, // 这里也要加上 <Renderer>
}

impl<'a> Program<Message, Theme, Renderer> for PianoRollGrid<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let geometry = self.cache.draw(renderer, bounds.size(),|frame|{
            // AI告诉我，第一步要把背景涂成深色，我暂时跳过
            //let background = Path::rectangle(Point::ORIGIN, frame.size());
            //frame.fill(&background, Color::from_rgb8(30, 30, 30));

            // 分别是横向和纵向
            self.draw_keys(frame, bounds);
            self.draw_bars(frame, bounds);
        });
        vec![geometry]
    }
}

impl<'a> PianoRollGrid<'a> {
    /// 绘制横向线，包括琴键分隔线
    fn draw_keys(&self, frame: &mut Frame<Renderer>, bounds: Rectangle) {
        // 视图状态
        let view = self.state;
        // 线条粗细和颜色
        let line_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(Color::from_rgb8(0x40, 0x40, 0x40));
        // 最高琴键索引
        let max_key_index = (view.key_count - 1) as f32;
        // 这里f32指的是iced绘图坐标，不是键号

        for i in 0..view.key_count {
            let keynum = i as isize;

            // 坐标计算
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;
            // 判断背景色

            // 绘制底部分割线
            let line_y = screen_y + view.zoom_y;
            let path = Path::line(
                Point::new(0.0, line_y),
                Point::new(bounds.width, line_y),
            );
            frame.stroke(&path, line_stroke);
        }
    }
    fn draw_bars(&self, frame: &mut Frame<Renderer>, bounds: Rectangle) {
        let view = self.state;
        let ppq = view.ppq as f32;

        // 这里只是随便写个四四拍，这个会根据歌曲变化
        let measure_ticks = ppq * 4.0;

        // 没看懂这是干啥的
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + bounds.width) / view.zoom_x;

        //更没看懂了
        let mut current_tick = (start_tick / ppq).ceil() * ppq;

        // 线条样式，但是我希望根据主题色有所变化
        let bar_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(Color::from_rgb8(80, 80, 80));
        let beat_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(Color::from_rgb8(40, 40, 40));

        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x;

            let is_measure = (current_tick % measure_ticks).abs() < 0.1;
            let stroke = if is_measure { bar_stroke } else { beat_stroke };

            let path = Path::line(
                Point::new(screen_x, 0.0),
                Point::new(screen_x, bounds.height)
            );
            frame.stroke(&path, stroke);

            current_tick += ppq;
        }
    }
}

/// 判断某个琴键是否该涂黑，它不是指这里是不是黑键，而是根据你选择什么调式灵活处理
fn is_key_dark(key: isize, _key_count: usize) -> bool {
    // 先这么写，这是12平均律
    let note_in_octave = key % 12;
    match note_in_octave {
        1 | 3 | 6 | 8 | 10 => true,
        _ => false,
    }
}
