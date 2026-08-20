//! 纵向卷帘只读预览视图（基础样式）
//!
//! 将横向卷帘的「时间轴」转置到 Y 轴：左侧是纵向小节标尺（样式与横向卷帘
//! 小节标尺一致），右侧是转置后的网格线（小节线 / 拍线逻辑与横向一致）。
//!
//! 本文件仅为「基本只读模式 UI 样式」：复用横向卷帘同款标尺 tick 计算与配色，
//! 先呈现布局与网格；音符瀑布流内容后续接入。

mod keyboard;

use iced_core::alignment;
use iced_core::mouse;
use iced_core::{Color, Length, Point, Rectangle, Size};
use iced_widget::canvas::{self, Canvas, Frame, Program};
use iced_widget::{column, container, row};

use crate::root::Root;
use crate::{Element, Message, Renderer, Theme};
use keyboard::VerticalKeyboardProgram;
use lumino_ui_editor::grid::theme::ThemeExt;

/// 纵向卷帘网格「每小节拍数」（基础样式暂固定 4/4，与横向卷帘空拍号回退一致）
const DEFAULT_NUMERATOR: u32 = 4;
/// 纵向卷帘网格「以几分音符为一拍」（4 = 四分音符，与 PPQ 定义一致）
const DEFAULT_DENOMINATOR: u32 = 4;

impl Root {
    /// 渲染纵向卷帘只读预览视图（替换钢琴卷帘画布区域）
    pub(crate) fn view_vertical_roll(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_vertical_roll");

        let view = &self.editor.editor_state.view;
        let ppq = view.ppq as u32;
        let zoom_x = view.zoom_x;
        let scroll_x = view.scroll_x;
        let ruler_width = view.ruler_height;
        let total_ticks = view.total_ticks;

        // 小节线 / 拍线 tick 计算（复用横向卷帘同款算法，仅转置到 Y 轴）
        let (beat_ticks, measure_ticks) = ticks_per_beat_and_measure(ppq);

        let program = VerticalRollProgram {
            zoom_x,
            scroll_x,
            ruler_width,
            total_ticks,
            beat_ticks,
            measure_ticks,
        };

        // 纵向键盘（编辑区底部，键沿 X 轴铺开，复用横向键盘同款 pitch 轴映射）
        let keyboard_program = VerticalKeyboardProgram {
            visible_key_count: view.visible_key_count,
            zoom_y: view.zoom_y,
            scroll_y: view.scroll_y,
            ruler_width,
        };
        let keyboard_height = ruler_width;

        // 顶部：左侧纵向小节标尺 + 右侧转置网格线（时间轴在 Y 轴）
        let top = row![
            Canvas::new(RulerProgram {
                zoom_x,
                scroll_x,
                ruler_width,
                total_ticks,
                beat_ticks,
                measure_ticks,
            })
            .width(Length::Fixed(ruler_width))
            .height(Length::Fill),
            Canvas::new(program)
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .height(Length::Fill);

        // 底部：纵向键盘（与上方网格 X 轴对齐）+ 左下空白（对齐左侧标尺列）
        let bottom = row![
            Canvas::new(keyboard_program)
                .width(Length::Fill)
                .height(Length::Fixed(keyboard_height)),
            container(iced_widget::Space::new())
                .width(Length::Fixed(ruler_width))
                .height(Length::Fixed(keyboard_height)),
        ]
        .height(Length::Fixed(keyboard_height));

        let content = column![top, bottom].height(Length::Fill);

        let background = container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(palette.background.base.color)
            });

        background.into()
    }
}

/// 每拍 / 每小节 tick 数（与 `lumino_gfx::grid` 同款公式）
fn ticks_per_beat_and_measure(ppq: u32) -> (u32, u32) {
    let beat_ticks = (ppq as f32 * 4.0 / DEFAULT_DENOMINATOR.max(1) as f32) as u32;
    let measure_ticks = beat_ticks * DEFAULT_NUMERATOR.max(1);
    (beat_ticks, measure_ticks)
}

/// tick → 纵向屏幕坐标（时间轴在 Y 轴；标尺宽度作为左侧留白）
fn tick_to_y(tick: f32, ruler_width: f32, zoom_x: f32, scroll_x: f32) -> f32 {
    ruler_width + tick * zoom_x - scroll_x
}

/// 纵向卷帘网格 Canvas —— 绘制转置后的小节线 / 拍线
struct VerticalRollProgram {
    zoom_x: f32,
    scroll_x: f32,
    ruler_width: f32,
    total_ticks: u32,
    beat_ticks: u32,
    measure_ticks: u32,
}

impl Program<Message, Theme, Renderer> for VerticalRollProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        _renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<iced_wgpu::Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());

        // 背景
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            theme.extended_palette().background.base.color,
        );

        let width = bounds.width;
        let height = bounds.height;
        let bar_color = theme.bar_line_color();
        let beat_color = theme.beat_line_color();

        // 小节线（每 measure_ticks 一条，横贯宽度）
        let first_measure = (self.scroll_x / (self.measure_ticks as f32)).floor() as i64 - 1;
        let last_measure = ((self.scroll_x + height - self.ruler_width)
            / (self.measure_ticks as f32))
            .ceil() as i64
            + 1;
        for m in first_measure..=last_measure {
            let tick = m as f32 * self.measure_ticks as f32;
            if tick < 0.0 || tick > self.total_ticks as f32 {
                continue;
            }
            let y = tick_to_y(tick, self.ruler_width, self.zoom_x, self.scroll_x);
            if y < self.ruler_width || y > height {
                continue;
            }
            frame.fill_rectangle(
                Point::new(0.0, y),
                Size::new(width, 1.0),
                Color {
                    a: 0.5,
                    ..bar_color
                },
            );
        }

        // 拍线（小节内细分，排除已画的小节线）
        let first_beat = (self.scroll_x / (self.beat_ticks as f32)).floor() as i64 - 1;
        let last_beat = ((self.scroll_x + height - self.ruler_width) / (self.beat_ticks as f32))
            .ceil() as i64
            + 1;
        for b in first_beat..=last_beat {
            let tick = b as f32 * self.beat_ticks as f32;
            if tick < 0.0 || tick > self.total_ticks as f32 {
                continue;
            }
            if (tick as i64 % self.measure_ticks as i64).abs() < f32::EPSILON as i64 {
                continue;
            }
            let y = tick_to_y(tick, self.ruler_width, self.zoom_x, self.scroll_x);
            if y < self.ruler_width || y > height {
                continue;
            }
            frame.fill_rectangle(
                Point::new(0.0, y),
                Size::new(width, 1.0),
                Color {
                    a: 0.25,
                    ..beat_color
                },
            );
        }

        vec![frame.into_geometry()]
    }
}

/// 纵向卷帘小节标尺 Canvas（位于左侧，样式对齐横向卷帘小节标尺）
struct RulerProgram {
    zoom_x: f32,
    scroll_x: f32,
    ruler_width: f32,
    total_ticks: u32,
    beat_ticks: u32,
    measure_ticks: u32,
}

impl Program<Message, Theme, Renderer> for RulerProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        _renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<iced_wgpu::Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());

        // 标尺底纹（比主区略深，区分内容区）
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            theme.extended_palette().background.weak.color,
        );

        let height = bounds.height;
        let bar_color = theme.bar_line_color();
        let text_color = theme.text_color();

        // 小节号 + 小节线刻度（标尺右侧边缘短横线）
        let first_measure = (self.scroll_x / (self.measure_ticks as f32)).floor() as i64 - 1;
        let last_measure = ((self.scroll_x + height - self.ruler_width)
            / (self.measure_ticks as f32))
            .ceil() as i64
            + 1;
        for m in first_measure..=last_measure {
            let tick = m as f32 * self.measure_ticks as f32;
            if tick < 0.0 || tick > self.total_ticks as f32 {
                continue;
            }
            let y = tick_to_y(tick, self.ruler_width, self.zoom_x, self.scroll_x);
            if y < self.ruler_width || y > height {
                continue;
            }
            // 标尺右侧刻度短线
            frame.fill_rectangle(
                Point::new(self.ruler_width - 6.0, y),
                Size::new(6.0, 1.0),
                Color {
                    a: 0.7,
                    ..bar_color
                },
            );
            // 小节号（沿 Y 轴排布，居中显示在标尺宽度内）
            frame.fill_text(canvas::Text {
                content: format!("{}", m + 1),
                position: Point::new(self.ruler_width / 2.0, y + 2.0),
                max_width: self.ruler_width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(11.0),
                color: Color {
                    a: 0.6,
                    ..text_color
                },
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            });
        }

        // 拍刻度（仅短标记，不标数字）
        let first_beat = (self.scroll_x / (self.beat_ticks as f32)).floor() as i64 - 1;
        let last_beat = ((self.scroll_x + height - self.ruler_width) / (self.beat_ticks as f32))
            .ceil() as i64
            + 1;
        for b in first_beat..=last_beat {
            let tick = b as f32 * self.beat_ticks as f32;
            if tick < 0.0 || tick > self.total_ticks as f32 {
                continue;
            }
            if (tick as i64 % self.measure_ticks as i64).abs() < f32::EPSILON as i64 {
                continue;
            }
            let y = tick_to_y(tick, self.ruler_width, self.zoom_x, self.scroll_x);
            if y < self.ruler_width || y > height {
                continue;
            }
            frame.fill_rectangle(
                Point::new(self.ruler_width - 3.0, y),
                Size::new(3.0, 1.0),
                Color {
                    a: 0.4,
                    ..bar_color
                },
            );
        }

        vec![frame.into_geometry()]
    }
}
