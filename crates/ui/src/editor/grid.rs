use crate::editor::{Editor, HitType};
use crate::{Message, Renderer, Theme, message::EditorAction};
use iced_core::{Point, Rectangle, mouse};

use iced_widget::canvas::{self, Event, Frame, Geometry, Path, Program, Stroke};

/// 钢琴卷帘网格绘制程序
pub struct PianoRollGrid<'a> {
    pub editor: &'a Editor,
}

impl<'a> PianoRollGrid<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

/// 存储 Canvas 状态的类型
pub struct CanvasState {
    /// 鼠标位置
    position: Option<iced_core::Point>,
    /// 上次点击时间（用于双击检测）
    last_click_time: std::time::Instant,
    /// 上次点击位置
    last_click_pos: Option<iced_core::Point>,
    /// Shift 键是否按下
    shift_pressed: bool,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            position: None,
            last_click_time: std::time::Instant::now(),
            last_click_pos: None,
            shift_pressed: false,
        }
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

                    // 检测双击（300ms 内两次点击且位置接近）
                    let now = std::time::Instant::now();
                    let is_double_click = if let Some(last_pos) = state.last_click_pos {
                        let time_delta = now.duration_since(state.last_click_time).as_millis();
                        let pos_delta = ((local_pos.x - last_pos.x).powi(2)
                            + (local_pos.y - last_pos.y).powi(2))
                        .sqrt();
                        time_delta < 300 && pos_delta < 10.0
                    } else {
                        false
                    };

                    if is_double_click {
                        // 双击事件
                        return Some(canvas::Action::publish(Message::EditorAction(
                            EditorAction::DoubleClicked(local_pos),
                        )));
                    } else {
                        // 单击事件
                        state.last_click_time = now;
                        state.last_click_pos = Some(local_pos);
                        return Some(canvas::Action::publish(Message::EditorAction(
                            EditorAction::Pressed {
                                pos: local_pos,
                                shift: state.shift_pressed,
                            },
                        )));
                    }
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
                let (delta_x, delta_y) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x * 30.0, *y * 30.0),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                // 限制最大滚动增量，避免滚动过快
                let max_delta = 100.0;
                let delta_x = delta_x.clamp(-max_delta, max_delta);
                let delta_y = delta_y.clamp(-max_delta, max_delta);
                return Some(canvas::Action::publish(Message::EditorAction(
                    EditorAction::Scrolled { delta_x, delta_y },
                )));
            }
            _ => {}
        }

        // 发送 bounds 变化消息
        Some(canvas::Action::publish(Message::CanvasBoundsChanged {
            offset: bounds_pos,
            size: bounds_size,
        }))
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
            if state.shift_pressed {
                // Shift按下时显示十字准星（框选模式）
                return mouse::Interaction::Crosshair;
            } else {
                // 普通橡皮擦模式显示指针
                return mouse::Interaction::Pointer;
            }
        }

        match self.editor.edit_state {
            EditState::Dragging { .. } => mouse::Interaction::Grabbing,
            EditState::PendingDrag { .. } => mouse::Interaction::Pointer,
            EditState::ResizingStart { .. } | EditState::ResizingEnd { .. } => {
                mouse::Interaction::ResizingHorizontally
            }
            EditState::Drawing { .. } => mouse::Interaction::Crosshair,
            EditState::Selecting { .. } => mouse::Interaction::Crosshair,
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
        let grid = self
            .editor
            .grid_cache
            .draw(renderer, bounds.size(), |frame| {
                self.draw_keyboard(frame, bounds, theme);
                self.draw_keys(frame, bounds, theme);
                self.draw_bars(frame, bounds, theme);
            });

        // 绘制框选框（不需要缓存，实时渲染）
        let mut geometries = vec![grid];

        if let Some(selection_geom) = self.draw_selection_box(renderer, theme, bounds) {
            geometries.push(selection_geom);
        }

        // 绘制远端鼠标游标
        for (pos, color_str, username) in self.editor.remote_cursors.values() {
            let color = parse_color(color_str).unwrap_or(iced_core::Color::WHITE);
            let mut frame = Frame::new(renderer, bounds.size());

            let cursor_x = pos.x;
            let cursor_y = pos.y;

            // 绘制游标线（贯穿整个高度）
            let path = Path::line(
                Point::new(cursor_x, 0.0),
                Point::new(cursor_x, bounds.height),
            );
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(1.5)
                    .with_color(iced_core::Color { a: 0.6, ..color }),
            );

            // 绘制鼠标指针（箭头形状）
            let arrow_size = 12.0;
            let arrow_path = Path::new(|builder| {
                // 箭头指向左上方
                builder.move_to(Point::new(cursor_x, cursor_y));
                builder.line_to(Point::new(cursor_x, cursor_y + arrow_size));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.5,
                    cursor_y + arrow_size * 0.8,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.8,
                    cursor_y + arrow_size * 1.5,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 1.2,
                    cursor_y + arrow_size * 1.2,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.9,
                    cursor_y + arrow_size * 0.5,
                ));
                builder.line_to(Point::new(cursor_x + arrow_size, cursor_y));
                builder.close();
            });
            frame.fill(&arrow_path, color);

            // 绘制白色边框使箭头更清晰
            let arrow_border = Path::new(|builder| {
                builder.move_to(Point::new(cursor_x, cursor_y));
                builder.line_to(Point::new(cursor_x, cursor_y + arrow_size));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.5,
                    cursor_y + arrow_size * 0.8,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.8,
                    cursor_y + arrow_size * 1.5,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 1.2,
                    cursor_y + arrow_size * 1.2,
                ));
                builder.line_to(Point::new(
                    cursor_x + arrow_size * 0.9,
                    cursor_y + arrow_size * 0.5,
                ));
                builder.line_to(Point::new(cursor_x + arrow_size, cursor_y));
                builder.close();
            });
            frame.stroke(
                &arrow_border,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(iced_core::Color::WHITE),
            );

            // 绘制用户名片背景
            let text_padding = 4.0;
            let username_len = username.len() as f32 * 7.0; // 估算文本宽度
            let label_width = username_len + text_padding * 2.0;
            let label_height = 18.0;
            let label_x = cursor_x + arrow_size + 4.0;
            let label_y = cursor_y - 2.0;

            let label_rect = Rectangle::new(
                Point::new(label_x, label_y),
                iced_core::Size::new(label_width, label_height),
            );
            let label_path = Path::rounded_rectangle(
                label_rect.position(),
                label_rect.size(),
                iced_core::border::Radius::from(4.0),
            );
            frame.fill(&label_path, color);

            // 绘制用户名文本
            let text = iced_widget::canvas::Text {
                content: username.clone(),
                position: Point::new(label_x + text_padding, label_y + 2.0),
                max_width: label_width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(11.0),
                color: iced_core::Color::WHITE,
                font: iced_core::Font::DEFAULT,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Top.into(),
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(text);

            geometries.push(frame.into_geometry());
        }

        geometries
    }
}

fn parse_color(hex: &str) -> Option<iced_core::Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(iced_core::Color::from_rgb8(r, g, b))
}

/// 网格绘制实现
impl<'a> PianoRollGrid<'a> {
    /// 绘制钢琴键盘（左侧键位指示器）
    fn draw_keyboard(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        let view = &self.editor.state;
        let keyboard_width = view.keyboard_width;
        let max_key_index = (view.visible_key_count - 1) as f32;

        // 根据主题亮暗选择合适的键盘颜色
        let is_light_theme = theme.extended_palette().background.weakest.color.r > 0.5;

        for i in 0..view.visible_key_count {
            let keynum = i as isize;
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y;

            if screen_y + view.zoom_y >= 0.0 && screen_y <= bounds.height {
                let is_black_key = is_key_dark(keynum, view.visible_key_count as usize);
                let key_color = if is_black_key {
                    if is_light_theme {
                        palette.strong.color
                    } else {
                        palette.base.color
                    }
                } else if is_light_theme {
                    palette.weak.color
                } else {
                    palette.weakest.color
                };

                let key_rect = Rectangle::new(
                    Point::new(0.0, screen_y),
                    iced_core::Size::new(keyboard_width, view.zoom_y),
                );
                let key_path = Path::rectangle(key_rect.position(), key_rect.size());
                frame.fill(&key_path, key_color);

                let border_stroke =
                    Stroke::default()
                        .with_width(1.0)
                        .with_color(if is_light_theme {
                            palette.strongest.color
                        } else {
                            palette.base.color
                        });
                frame.stroke(&key_path, border_stroke);
            }
        }
    }

    /// 绘制琴键分隔线（横向线）
    fn draw_keys(&self, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
        let palette = theme.extended_palette().background;
        let view = &self.editor.state;

        // 根据主题亮暗选择合适的线条颜色
        let line_color = if theme.extended_palette().background.weakest.color.r > 0.5 {
            // 亮色主题：使用较深的颜色
            palette.strong.color
        } else {
            // 暗色主题：使用较浅的颜色
            palette.weak.color
        };

        let line_stroke = Stroke::default().with_width(1.0).with_color(line_color);

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
        let view = &self.editor.state;
        let ppq = view.ppq as f32;
        let palette = theme.extended_palette().background;
        let keyboard_width = view.keyboard_width;

        let measure_ticks = ppq * 4.0;
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

        // 网格线间隔：ppq/4 = 480 ticks
        let grid_gap = ppq / 4.0;
        let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

        // 根据主题亮暗选择合适的线条颜色
        let is_light_theme = theme.extended_palette().background.weakest.color.r > 0.5;

        let bar_stroke = Stroke::default()
            .with_width(4.0)
            .with_color(if is_light_theme {
                palette.strongest.color
            } else {
                palette.base.color
            });
        let beat_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(if is_light_theme {
                palette.strong.color
            } else {
                palette.weak.color
            });
        let sub_beat_stroke = Stroke::default()
            .with_width(0.5)
            .with_color(if is_light_theme {
                palette.strong.color
            } else {
                palette.weaker.color
            });
        // 后面留一个api，让用户自己可以设置

        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x + keyboard_width;

            if screen_x >= keyboard_width && screen_x <= bounds.width {
                let is_measure = (current_tick % measure_ticks).abs() < 0.1;
                let is_beat = (current_tick % ppq).abs() < 0.1;
                let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

                let stroke = if is_measure {
                    bar_stroke
                } else if is_beat {
                    beat_stroke
                } else if is_half_beat {
                    sub_beat_stroke
                } else {
                    Stroke::default()
                        .with_width(0.5)
                        .with_color(iced_core::Color {
                            a: 0.1,
                            ..palette.weaker.color
                        })
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

/// 框选框绘制
impl<'a> PianoRollGrid<'a> {
    fn draw_selection_box(
        &self,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
    ) -> Option<Geometry<Renderer>> {
        use iced_widget::canvas::{self, Path, Stroke};

        let (start_pos, current_pos) = self.editor.get_selection_box()?;

        // 计算选择框的位置和尺寸
        let min_x = start_pos.x.min(current_pos.x);
        let max_x = start_pos.x.max(current_pos.x);
        let min_y = start_pos.y.min(current_pos.y);
        let max_y = start_pos.y.max(current_pos.y);

        let width = max_x - min_x;
        let height = max_y - min_y;

        if width < 1.0 || height < 1.0 {
            return None;
        }

        let palette = theme.extended_palette();
        let selection_color = palette.secondary.strong.color;

        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // 绘制填充（半透明）
        let rect = Rectangle::new(
            Point::new(min_x, min_y),
            iced_core::Size::new(width, height),
        );
        let path = Path::rectangle(rect.position(), rect.size());

        let fill_color = iced_core::Color {
            r: selection_color.r,
            g: selection_color.g,
            b: selection_color.b,
            a: 0.2,
        };
        frame.fill(&path, fill_color);

        // 绘制边框
        let stroke = Stroke::default()
            .with_width(1.0)
            .with_color(selection_color);
        frame.stroke(&path, stroke);

        Some(frame.into_geometry())
    }
}
