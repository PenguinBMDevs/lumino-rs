//! Pattern Widget —— 音轨总览中的音符片段可视化与交互
//!
//! 功能：
//! - 灰色圆角矩形背景 + 蓝色音符缩略图
//! - 左右拖动手柄调整 start_tick / length
//! - 左上角显示 Pattern 名称
//! - 悬停高亮边缘

use std::f32;

use iced_core::{Color, Point, Rectangle, Size, alignment, mouse};
use iced_widget::canvas::{self, Frame, Program, path};

use crate::{Message, Renderer, Theme};
use lumino_ui_core::message::PatternAction;

// iced_wgpu::Geometry is the concrete canvas geometry type for the wgpu backend
use iced_wgpu::Geometry as Geom;

use lumino_core::Pattern;

use super::ArrangementViewport;

// ============================================================
// 常量
// ============================================================

/// 边缘拖拽检测范围（像素）
const EDGE_HIT_WIDTH: f32 = 10.0;
/// 拖动手柄宽度（像素）
const HANDLE_WIDTH: f32 = 6.0;
/// 圆角半径（背景）
const BG_CORNER_RADIUS: f32 = 26.0;
/// 音符条圆角半径
const NOTE_BAR_RADIUS: f32 = 3.0;
/// 内边距（上下左右）
const PADDING: f32 = 8.0;
/// 名称文字大小（像素）
const LABEL_FONT_SIZE: f32 = 11.0;
/// 音符缩略图高度占比
const NOTE_HEIGHT_RATIO: f32 = 0.5;
/// 音符缩略图最小间距
const NOTE_MIN_SPACING: f32 = 4.0;
/// 默认音符缩略图数量
const DEFAULT_NOTE_COUNT: usize = 5;

// ============================================================
// 状态结构体
// ============================================================

/// Pattern Widget 交互状态
#[derive(Debug, Default)]
pub struct PatternWidgetState {
    /// 是否正在拖拽左侧边缘
    pub dragging_left: bool,
    /// 是否正在拖拽右侧边缘
    pub dragging_right: bool,
    /// 拖拽起始 X 坐标（屏幕坐标）
    pub drag_start_x: f32,
    /// 拖拽起始时的值（左侧为 start_tick，右侧为 length）
    pub drag_start_value: f32,
    /// 鼠标是否悬停在左边缘
    pub hover_left: bool,
    /// 鼠标是否悬停在右边缘
    pub hover_right: bool,
}

// ============================================================
// Widget 结构体
// ============================================================

/// Pattern Canvas 绘制程序
pub struct PatternWidget<'a> {
    /// 引用的 Pattern 数据
    pub pattern: &'a Pattern,
    /// 视口信息，用于坐标转换
    pub viewport: &'a ArrangementViewport,
}

impl<'a> PatternWidget<'a> {
    /// 创建新的 PatternWidget
    pub fn new(pattern: &'a Pattern, viewport: &'a ArrangementViewport) -> Self {
        Self { pattern, viewport }
    }

    /// 将屏幕 X 坐标转换为 tick 值
    fn screen_x_to_tick(screen_x: f32, zoom_x: f32) -> f32 {
        if zoom_x.abs() < f32::EPSILON {
            return 0.0;
        }
        screen_x / zoom_x
    }

    /// 判断坐标是否在左边缘区域
    fn is_in_left_edge(local_x: f32, _bounds_width: f32) -> bool {
        local_x <= EDGE_HIT_WIDTH
    }

    /// 判断坐标是否在右边缘区域
    fn is_in_right_edge(local_x: f32, bounds_width: f32) -> bool {
        local_x >= bounds_width - EDGE_HIT_WIDTH
    }
}

// ============================================================
// Program trait 实现
// ============================================================

impl Program<Message, Theme, Renderer> for PatternWidget<'_> {
    type State = PatternWidgetState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let bounds_size = bounds.size();

        if bounds_size.width <= 1.0 || bounds_size.height <= 1.0 {
            return None;
        }

        let cursor_pos = match cursor.position() {
            Some(pos) => Point::new(pos.x - bounds.x, pos.y - bounds.y),
            None => return None,
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // 检测点击位置：左边缘、右边缘或主体
                if Self::is_in_left_edge(cursor_pos.x, bounds_size.width) {
                    state.dragging_left = true;
                    state.drag_start_x = cursor.position().unwrap_or_default().x;
                    state.drag_start_value = self.pattern.start_tick;
                    tracing::debug!(
                        "[PatternWidget] 开始拖拽左边缘, start_tick={}",
                        self.pattern.start_tick
                    );
                    return Some(canvas::Action::publish(Message::Pattern(
                        PatternAction::DragStartLeft(self.pattern.id),
                    )));
                }

                if Self::is_in_right_edge(cursor_pos.x, bounds_size.width) {
                    state.dragging_right = true;
                    state.drag_start_x = cursor.position().unwrap_or_default().x;
                    state.drag_start_value = self.pattern.length;
                    tracing::debug!(
                        "[PatternWidget] 开始拖拽右边缘, length={}",
                        self.pattern.length
                    );
                    return Some(canvas::Action::publish(Message::Pattern(
                        PatternAction::DragStartRight(self.pattern.id),
                    )));
                }

                // 点击 Pattern 主体 → 选中
                Some(canvas::Action::publish(Message::Pattern(
                    PatternAction::Selected(self.pattern.id),
                )))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // 更新悬停状态
                state.hover_left = Self::is_in_left_edge(cursor_pos.x, bounds_size.width);
                state.hover_right = Self::is_in_right_edge(cursor_pos.x, bounds_size.width);

                // 左边缘拖拽中：更新 start_tick
                if state.dragging_left {
                    let abs_cursor_x = cursor.position().unwrap_or_default().x;
                    let delta_x = abs_cursor_x - state.drag_start_x;
                    let delta_tick = Self::screen_x_to_tick(delta_x, self.viewport.zoom_x);
                    let new_start_tick = (state.drag_start_value + delta_tick).max(0.0);
                    return Some(canvas::Action::publish(Message::Pattern(
                        PatternAction::DragMoveLeft(self.pattern.id, new_start_tick),
                    )));
                }

                // 右边缘拖拽中：更新 length
                if state.dragging_right {
                    let abs_cursor_x = cursor.position().unwrap_or_default().x;
                    let delta_x = abs_cursor_x - state.drag_start_x;
                    let delta_tick = Self::screen_x_to_tick(delta_x, self.viewport.zoom_x);
                    let new_length = (state.drag_start_value + delta_tick).max(1.0);
                    return Some(canvas::Action::publish(Message::Pattern(
                        PatternAction::DragMoveRight(self.pattern.id, new_length),
                    )));
                }

                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let was_dragging = state.dragging_left || state.dragging_right;
                state.dragging_left = false;
                state.dragging_right = false;
                state.drag_start_x = 0.0;
                state.drag_start_value = 0.0;

                if was_dragging {
                    tracing::debug!("[PatternWidget] 结束拖拽");
                    return Some(canvas::Action::publish(Message::Pattern(
                        PatternAction::DragEnd,
                    )));
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geom> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.size();

        // 1. 绘制灰色圆角矩形背景
        draw_background(&mut frame, size);

        // 2. 绘制蓝色音符缩略图
        draw_note_bars(&mut frame, size, &self.pattern.color);

        // 3. 绘制左右拖动手柄
        draw_handles(&mut frame, size);

        // 4. 绘制 Pattern 名称
        draw_label(&mut frame, size, &self.pattern.name);

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_left || state.dragging_right {
            return mouse::Interaction::ResizingHorizontally;
        }

        if let Some(cursor_pos) = cursor.position() {
            let local_x = cursor_pos.x - _bounds.x;
            if Self::is_in_left_edge(local_x, _bounds.width)
                || Self::is_in_right_edge(local_x, _bounds.width)
            {
                return mouse::Interaction::ResizingHorizontally;
            }
        }

        if state.hover_left || state.hover_right {
            return mouse::Interaction::ResizingHorizontally;
        }

        mouse::Interaction::default()
    }
}

// ============================================================
// 绘制函数
// ============================================================

/// 绘制灰色圆角矩形背景
fn draw_background(frame: &mut Frame<Renderer>, size: Size) {
    let bg_path =
        path::Path::rounded_rectangle(Point::new(0.0, 0.0), size, BG_CORNER_RADIUS.into());
    let bg_color = Color::from_rgb(0.851, 0.851, 0.851); // #d9d9d9
    frame.fill(&bg_path, bg_color);
}

/// 绘制蓝色音符缩略图（模拟音符分布的小矩形）
fn draw_note_bars(frame: &mut Frame<Renderer>, size: Size, color: &[f32; 4]) {
    let inner_width = size.width - PADDING * 2.0;
    let inner_height = size.height - PADDING * 2.0;
    let note_height = inner_height * NOTE_HEIGHT_RATIO;
    let note_y = PADDING + (inner_height - note_height) / 2.0;

    if inner_width <= NOTE_MIN_SPACING || note_height <= 0.0 {
        return;
    }

    // 根据宽度动态决定音符条数量和间距
    let spacing = NOTE_MIN_SPACING;
    let bar_width = (inner_width - spacing * (DEFAULT_NOTE_COUNT - 1) as f32)
        .max(NOTE_MIN_SPACING)
        .min(inner_width / DEFAULT_NOTE_COUNT as f32);

    let note_color = Color::from_rgba(color[0], color[1], color[2], color[3]);

    for i in 0..DEFAULT_NOTE_COUNT {
        let x = PADDING + i as f32 * (bar_width + spacing);
        // 随机化每个音符条的微小高度差异，增加视觉层次感
        let height_var = match i % 3 {
            0 => note_height,
            1 => note_height * 0.7,
            _ => note_height * 0.85,
        };
        let y_offset = (note_height - height_var) / 2.0;
        let bar_y = note_y + y_offset;

        let bar_path = path::Path::rounded_rectangle(
            Point::new(x, bar_y),
            Size::new(bar_width, height_var),
            NOTE_BAR_RADIUS.into(),
        );
        frame.fill(&bar_path, note_color);
    }
}

/// 绘制左右拖动手柄（灰色窄矩形）
fn draw_handles(frame: &mut Frame<Renderer>, size: Size) {
    let handle_color = Color::from_rgb(0.455, 0.455, 0.455); // #747474
    let handle_height = size.height;

    // 左侧手柄
    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        Size::new(HANDLE_WIDTH, handle_height),
        handle_color,
    );

    // 右侧手柄
    frame.fill_rectangle(
        Point::new(size.width - HANDLE_WIDTH, 0.0),
        Size::new(HANDLE_WIDTH, handle_height),
        handle_color,
    );
}

/// 绘制 Pattern 名称文本（左上角）
fn draw_label(frame: &mut Frame<Renderer>, size: Size, name: &str) {
    let text_color = Color::from_rgba(0.2, 0.2, 0.2, 0.9);
    let label = canvas::Text {
        content: name.to_string(),
        position: Point::new(HANDLE_WIDTH + 4.0, 4.0),
        max_width: size.width - HANDLE_WIDTH * 2.0 - 8.0,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(LABEL_FONT_SIZE),
        color: text_color,
        font: iced_core::Font::DEFAULT,
        align_x: alignment::Horizontal::Left.into(),
        align_y: alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Basic,
    };
    frame.fill_text(label);
}
