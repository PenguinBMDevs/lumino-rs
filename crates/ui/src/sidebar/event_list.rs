//! 事件列表视图
//!
//! 使用 Canvas 虚拟绘制当前轨道的音符事件表格，避免为大列表构造大量 widget。
//! 数据源为外部传入的 `&im::Vector<Note>` 引用，不持有第二份拷贝。

use iced_core::{Length, Padding, Rectangle};
use iced_widget::canvas::{self, Frame, Geometry, Program, Text};
use iced_widget::{container, scrollable};
use lumino_core::im::Vector;
use lumino_core::note::{Note, note_name};

use crate::editor::grid::theme::ThemeExt;

use crate::sidebar::Event;
use crate::{Element, Message, Renderer, Theme};

/// 行高（像素）
const ROW_HEIGHT: f32 = 18.0;
/// 表头高度（像素）
const HEADER_HEIGHT: f32 = 20.0;
/// 字号
const FONT_SIZE: f32 = 11.0;
/// 列宽定义
const COL_WIDTHS: [f32; 6] = [36.0, 52.0, 42.0, 82.0, 46.0, 52.0];
/// 列标题
const COL_HEADERS: &[&str] = &["Mea", "Tick", "Step", "Event", "Gate", "Vel/Value"];

/// 事件列表 Canvas 状态（当前无需额外运行时状态）
#[derive(Debug, Default)]
pub struct EventListState;

/// 事件列表 Canvas
pub struct EventListCanvas<'a> {
    /// 当前轨道音符集合（零拷贝引用）
    notes: &'a Vector<Note>,
    /// 当前 PPQ
    ppq: u16,
    /// 吸附精度，作为 Step 列显示值
    snap_precision: f32,
    /// 当前垂直滚动偏移（由外部 scrollable 同步）
    scroll_y: f32,
    /// 可视区域高度（由 scrollable 回调提供）
    viewport_height: f32,
}

impl<'a> EventListCanvas<'a> {
    pub fn new(
        notes: &'a Vector<Note>,
        ppq: u16,
        snap_precision: f32,
        scroll_y: f32,
        viewport_height: f32,
    ) -> Self {
        Self {
            notes,
            ppq,
            snap_precision,
            scroll_y,
            viewport_height,
        }
    }

    /// 内容总高度
    fn total_height(&self) -> f32 {
        HEADER_HEIGHT + self.notes.len() as f32 * ROW_HEIGHT
    }

    /// 计算可见行范围（包含表头）
    fn visible_range(&self, scroll_y: f32, viewport_height: f32) -> (usize, usize) {
        if self.notes.is_empty() {
            return (0, 0);
        }
        // 表头也随内容滚动，因此以表头底边越过视口顶部为第 0 行开始滚动的临界点
        let first = ((scroll_y - HEADER_HEIGHT) / ROW_HEIGHT).floor().max(0.0) as usize;
        // 表头仍占用部分视口时，实际可用于显示数据行的高度需扣除剩余表头
        let header_remaining = (HEADER_HEIGHT - scroll_y).max(0.0);
        let effective_viewport = (viewport_height - header_remaining).max(0.0);
        // 保守估计可见行数：ceil(可用高度/行高) + 1 用于覆盖顶部/底部被截断的行
        let visible_count = (effective_viewport / ROW_HEIGHT).ceil() as usize + 1;
        let last = (first + visible_count).min(self.notes.len());
        (first, last)
    }
}

impl<'a> Program<Message, Theme, Renderer> for EventListCanvas<'a> {
    type State = EventListState;

    fn update(
        &self,
        _state: &mut EventListState,
        _event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        None
    }

    fn draw(
        &self,
        _state: &EventListState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let canvas_w = bounds.size().width;
        let canvas_h = bounds.size().height;
        let palette = theme.extended_palette();
        let is_light = theme.is_light();

        // 背景
        frame.fill_rectangle(
            iced_core::Point::new(0.0, 0.0),
            iced_core::Size::new(canvas_w, canvas_h),
            palette.background.base.color,
        );

        // 表头背景
        let header_color = if is_light {
            iced_core::Color::from_rgb(0.92, 0.92, 0.92)
        } else {
            iced_core::Color::from_rgb(0.18, 0.18, 0.18)
        };
        frame.fill_rectangle(
            iced_core::Point::new(0.0, 0.0),
            iced_core::Size::new(canvas_w, HEADER_HEIGHT),
            header_color,
        );

        let text_color = palette.background.base.text;
        let header_text_color = if is_light {
            iced_core::Color::from_rgb(0.2, 0.2, 0.2)
        } else {
            iced_core::Color::from_rgb(0.85, 0.85, 0.85)
        };

        // 绘制表头文字
        let mut x = 4.0_f32;
        for (i, &header) in COL_HEADERS.iter().enumerate() {
            frame.fill_text(Text {
                content: header.to_string(),
                position: iced_core::Point::new(x, HEADER_HEIGHT * 0.5),
                color: header_text_color,
                size: iced_core::Pixels(FONT_SIZE),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                    FONT_SIZE * 1.2,
                )),
                font: iced_core::Font::default(),
                max_width: COL_WIDTHS[i],
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Basic,
            });
            x += COL_WIDTHS[i];
        }

        // 可见行范围：优先使用 scrollable 报告的视口高度，未收到时回退到 canvas 高度
        let viewport_height = if self.viewport_height > 0.0 {
            self.viewport_height
        } else {
            canvas_h
        };
        let (first, last) = self.visible_range(self.scroll_y, viewport_height);

        // 每小节 tick 数（默认 4/4 拍）
        let ticks_per_measure = self.ppq as f32 * 4.0;

        for idx in first..last {
            let Some(note) = self.notes.get(idx) else {
                continue;
            };
            let y = HEADER_HEIGHT + idx as f32 * ROW_HEIGHT - self.scroll_y;
            if y + ROW_HEIGHT < 0.0 || y > canvas_h {
                continue;
            }

            // 行交替背景
            if idx % 2 == 0 {
                let row_bg = if is_light {
                    iced_core::Color::from_rgb(0.97, 0.97, 0.97)
                } else {
                    iced_core::Color::from_rgb(0.22, 0.22, 0.22)
                };
                frame.fill_rectangle(
                    iced_core::Point::new(0.0, y),
                    iced_core::Size::new(canvas_w, ROW_HEIGHT),
                    row_bg,
                );
            }

            let measure = (note.tick / ticks_per_measure).floor() as i32 + 1;
            let tick_in_measure = note.tick - (measure as f32 - 1.0) * ticks_per_measure;
            let event_label = format!("{} [{}]", note_name(note.key), note.key);

            let values: [String; 6] = [
                format!("{}", measure),
                format!("{:.0}", tick_in_measure),
                format!("{:.0}", self.snap_precision),
                event_label,
                format!("{:.0}", note.length),
                format!("{}", note.velocity),
            ];

            let mut x = 4.0_f32;
            for (i, value) in values.iter().enumerate() {
                frame.fill_text(Text {
                    content: value.clone(),
                    position: iced_core::Point::new(x, y + ROW_HEIGHT * 0.5),
                    color: text_color,
                    size: iced_core::Pixels(FONT_SIZE),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        FONT_SIZE * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: COL_WIDTHS[i],
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Basic,
                });
                x += COL_WIDTHS[i];
            }
        }

        // 表头底部和列分隔线
        let line_color = if is_light {
            iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.15)
        } else {
            iced_core::Color::from_rgba(1.0, 1.0, 1.0, 0.08)
        };
        let mut path = iced_widget::canvas::path::Builder::new();
        // 表头底部分隔线
        path.move_to(iced_core::Point::new(0.0, HEADER_HEIGHT - 1.0));
        path.line_to(iced_core::Point::new(canvas_w, HEADER_HEIGHT - 1.0));
        // 垂直分隔线
        let mut x = 0.0_f32;
        for &w in &COL_WIDTHS[..COL_WIDTHS.len() - 1] {
            x += w;
            path.move_to(iced_core::Point::new(x, 0.0));
            path.line_to(iced_core::Point::new(x, canvas_h));
        }
        frame.stroke(
            &path.build(),
            iced_widget::canvas::Stroke::default()
                .with_color(line_color)
                .with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}

/// 渲染事件列表视图
pub fn view_event_list<'a>(
    notes: &'a Vector<Note>,
    ppq: u16,
    snap_precision: f32,
    scroll_y: f32,
    viewport_height: f32,
) -> Element<'a> {
    let canvas = EventListCanvas::new(notes, ppq, snap_precision, scroll_y, viewport_height);
    let total_height = canvas.total_height();

    let content = iced_widget::canvas::Canvas::new(canvas)
        .width(Length::Fixed(COL_WIDTHS.iter().sum::<f32>() + 8.0))
        .height(Length::Fixed(total_height.max(1.0)));

    let scrollable_content = scrollable(container(content).padding(Padding::ZERO))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(8).scroller_width(6),
        ))
        .on_scroll(|viewport| {
            Event::event_list_scrolled(viewport.absolute_offset().y, viewport.bounds().height)
        })
        .height(Length::Fill);

    container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_notes() -> Vector<Note> {
        let mut notes = Vector::new();
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
        notes.push_back(Note::new(480.0, 62, 480.0).with_velocity(90));
        notes.push_back(Note::new(960.0, 64, 240.0).with_velocity(80));
        notes
    }

    #[test]
    fn test_total_height() {
        let notes = sample_notes();
        let canvas = EventListCanvas::new(&notes, 480, 120.0, 0.0, 100.0);
        assert_eq!(canvas.total_height(), HEADER_HEIGHT + 3.0 * ROW_HEIGHT);
    }

    #[test]
    fn test_visible_range_empty() {
        let empty = Vector::new();
        let canvas = EventListCanvas::new(&empty, 480, 120.0, 0.0, 100.0);
        assert_eq!(canvas.visible_range(0.0, 100.0), (0, 0));
    }

    #[test]
    fn test_visible_range_clamped() {
        let notes = sample_notes();
        let canvas = EventListCanvas::new(&notes, 480, 120.0, 0.0, 36.0);
        // 视口高度 36px：扣除 20px 表头后剩余 16px，约 1 行可见，含过度绘制共 2 行
        let (first, last) = canvas.visible_range(0.0, 36.0);
        assert_eq!(first, 0);
        assert_eq!(last, 2);
    }

    #[test]
    fn test_visible_range_scrolled() {
        let notes = sample_notes();
        let canvas = EventListCanvas::new(&notes, 480, 120.0, ROW_HEIGHT, ROW_HEIGHT * 2.0);
        // 滚动超过表高 + 一行后，first 至少为 1
        let scroll_y = HEADER_HEIGHT + ROW_HEIGHT;
        let (first, last) = canvas.visible_range(scroll_y, ROW_HEIGHT * 2.0);
        assert!(first >= 1);
        assert!(last <= notes.len());
    }
}
