//! 工程走带左侧音轨列表 Canvas —— 按 yinhe 风格绘制音轨名称和选中状态
//!
//! 与右侧走带 Canvas 共享 scroll_y，实现同步滚动。

use std::collections::HashSet;
use std::time::Instant;

use iced_core::{Color, Point, Rectangle, Size, keyboard};
use iced_widget::canvas::{self, Frame, Geometry, Program, Stroke, Text};
use lumino_ui_core::color::{blend_color, contrast_text_color};

use crate::grid::theme::ThemeExt;
use crate::{Message, Renderer, Theme};

const BADGE_WIDTH: f32 = 8.0;
const TEXT_MARGIN: f32 = 6.0;
const BTN_SIZE: f32 = 18.0;
const BTN_GAP: f32 = 2.0;

/// 工程走带左侧音轨列表 Canvas
pub struct TrackListCanvas {
    /// 音轨列表：(id, name)
    pub tracks: Vec<(usize, String)>,
    /// 每轨显示标签（如 A01），与 tracks 一一对应
    pub track_labels: Vec<String>,
    /// 每轨通道号（用于生成显示标签）
    pub track_channels: Vec<u8>,
    /// 每轨颜色标签
    pub track_colors: Vec<Option<Color>>,
    /// 每轨是否为主控音轨
    pub track_conductors: Vec<bool>,
    /// 每轨静音状态（初始值）
    pub track_muted: Vec<bool>,
    /// 每轨独奏状态（初始值）
    pub track_soloed: Vec<bool>,
    /// 当前选中的音轨 ID（单选兼容）
    pub selected_track: usize,
    /// 当前多选集合（外部传入的初始值）
    pub selected_tracks: HashSet<usize>,
    /// 范围选择锚点
    pub selection_anchor: Option<usize>,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 每轨高度
    pub track_height: f32,
    /// 总高度
    pub total_height: f32,
}

/// 运行时交互状态
#[derive(Debug)]
pub struct TrackListState {
    /// 运行时静音状态
    pub track_muted: Vec<bool>,
    /// 运行时独奏状态
    pub track_soloed: Vec<bool>,
    /// 多选集合
    pub selected_tracks: HashSet<usize>,
    /// 范围选择锚点（tracks 数组索引）
    pub selection_anchor: Option<usize>,
    /// 当前修饰键
    pub modifiers: keyboard::Modifiers,
    /// 上次左键点击时间
    pub last_click_time: Instant,
    /// 上次左键点击位置
    pub last_click_pos: Option<Point>,
}

impl Default for TrackListState {
    fn default() -> Self {
        Self {
            track_muted: Vec::new(),
            track_soloed: Vec::new(),
            selected_tracks: HashSet::new(),
            selection_anchor: None,
            modifiers: keyboard::Modifiers::default(),
            last_click_time: Instant::now(),
            last_click_pos: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MuteSoloButton {
    Mute,
    Solo,
}

impl TrackListCanvas {
    pub fn new(
        tracks: Vec<(usize, String)>,
        selected_track: usize,
        scroll_y: f32,
        track_height: f32,
        total_height: f32,
    ) -> Self {
        let count = tracks.len();
        Self {
            tracks,
            track_labels: vec![String::new(); count],
            track_channels: vec![0; count],
            track_colors: vec![None; count],
            track_conductors: vec![false; count],
            track_muted: vec![false; count],
            track_soloed: vec![false; count],
            selected_track,
            selected_tracks: HashSet::new(),
            selection_anchor: None,
            scroll_y,
            track_height,
            total_height,
        }
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.track_labels = labels;
        self
    }

    pub fn with_channels(mut self, channels: Vec<u8>) -> Self {
        self.track_channels = channels;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Option<Color>>) -> Self {
        self.track_colors = colors;
        self
    }

    pub fn with_conductors(mut self, conductors: Vec<bool>) -> Self {
        self.track_conductors = conductors;
        self
    }

    pub fn with_mutes(mut self, muted: Vec<bool>) -> Self {
        self.track_muted = muted;
        self
    }

    pub fn with_solos(mut self, soloed: Vec<bool>) -> Self {
        self.track_soloed = soloed;
        self
    }

    pub fn with_selection(mut self, selected: HashSet<usize>, anchor: Option<usize>) -> Self {
        self.selected_tracks = selected;
        self.selection_anchor = anchor;
        self
    }

    fn ensure_state(&self, state: &mut TrackListState) {
        let count = self.tracks.len();
        state.track_muted.resize(count, false);
        state.track_soloed.resize(count, false);
        for (i, &v) in self.track_muted.iter().enumerate().take(count) {
            state.track_muted[i] = v;
        }
        for (i, &v) in self.track_soloed.iter().enumerate().take(count) {
            state.track_soloed[i] = v;
        }
        if state.selected_tracks.is_empty() {
            if !self.selected_tracks.is_empty() {
                state.selected_tracks.clone_from(&self.selected_tracks);
                state.selection_anchor = self.selection_anchor;
            } else {
                state.selected_tracks.insert(self.selected_track);
            }
        }
    }

    fn track_index_at_y(&self, y: f32) -> Option<usize> {
        let idx = (y / self.track_height) as usize;
        if idx < self.tracks.len() {
            Some(idx)
        } else {
            None
        }
    }

    fn is_mute_solo_hit(&self, pos: Point, idx: usize, canvas_w: f32) -> Option<MuteSoloButton> {
        if self.track_conductors.get(idx).copied().unwrap_or(false) {
            return None;
        }
        let track_y = idx as f32 * self.track_height - self.scroll_y;
        let total_btn_w = 2.0 * BTN_SIZE + BTN_GAP;
        let btn_x_start = canvas_w - total_btn_w - 6.0;
        let btn_y = track_y + (self.track_height - BTN_SIZE) * 0.5;
        if pos.x < btn_x_start
            || pos.x > btn_x_start + total_btn_w
            || pos.y < btn_y
            || pos.y > btn_y + BTN_SIZE
        {
            return None;
        }
        if pos.x < btn_x_start + BTN_SIZE {
            Some(MuteSoloButton::Mute)
        } else {
            Some(MuteSoloButton::Solo)
        }
    }

    fn handle_left_press(
        &self,
        state: &mut TrackListState,
        pos: Point,
        canvas_w: f32,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_constants::editor::{DOUBLE_CLICK_DISTANCE_PX, DOUBLE_CLICK_TIME_MS};

        let rel_y = pos.y + self.scroll_y;
        let idx = self.track_index_at_y(rel_y)?;
        let (track_id, _) = self.tracks.get(idx)?;
        let track_id = *track_id;

        let now = Instant::now();
        let is_double = state.last_click_pos.is_some_and(|last_pos| {
            let dt = now.duration_since(state.last_click_time).as_millis();
            let dist = ((pos.x - last_pos.x).powi(2) + (pos.y - last_pos.y).powi(2)).sqrt();
            dt < DOUBLE_CLICK_TIME_MS && dist < DOUBLE_CLICK_DISTANCE_PX
        });

        if is_double {
            return Some(canvas::Action::publish(
                lumino_ui_core::sidebar_event::Event::track_selected(track_id),
            ));
        }

        state.last_click_time = now;
        state.last_click_pos = Some(pos);

        if let Some(btn) = self.is_mute_solo_hit(pos, idx, canvas_w) {
            return Some(match btn {
                MuteSoloButton::Mute => {
                    if let Some(v) = state.track_muted.get_mut(idx) {
                        *v = !*v;
                    }
                    canvas::Action::publish(
                        lumino_ui_core::sidebar_event::Event::track_mute_toggled(track_id),
                    )
                }
                MuteSoloButton::Solo => {
                    if let Some(v) = state.track_soloed.get_mut(idx) {
                        *v = !*v;
                    }
                    canvas::Action::publish(
                        lumino_ui_core::sidebar_event::Event::track_solo_toggled(track_id),
                    )
                }
            });
        }

        let shift = state.modifiers.shift();

        if shift {
            if let Some(anchor_idx) = state.selection_anchor {
                let lo = anchor_idx.min(idx);
                let hi = anchor_idx.max(idx);
                state.selected_tracks.clear();
                for i in lo..=hi {
                    if let Some((id, _)) = self.tracks.get(i) {
                        state.selected_tracks.insert(*id);
                    }
                }
            } else {
                state.selected_tracks.clear();
                state.selected_tracks.insert(track_id);
            }
            state.selection_anchor = Some(idx);
        } else {
            state.selected_tracks.clear();
            state.selected_tracks.insert(track_id);
            state.selection_anchor = Some(idx);
        }

        let ids: Vec<usize> = state.selected_tracks.iter().copied().collect();
        Some(canvas::Action::publish(Message::Batch(vec![
            lumino_ui_core::sidebar_event::Event::track_selected(track_id),
            lumino_ui_core::sidebar_event::Event::tracks_selected(ids),
        ])))
    }
}

impl Program<Message, Theme, Renderer> for TrackListCanvas {
    type State = TrackListState;

    fn update(
        &self,
        state: &mut TrackListState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        puffin::profile_function!();
        self.ensure_state(state);

        match event {
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta }) => {
                use lumino_ui_constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};
                let (_, dy) = match delta {
                    iced_core::mouse::ScrollDelta::Lines { x, y } => {
                        (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                    }
                    iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                let dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                Some(canvas::Action::publish(Message::ArrangementScrollY(
                    self.scroll_y - dy,
                )))
            }
            canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
                iced_core::mouse::Button::Left,
            )) => {
                if let Some(pos) = cursor.position() {
                    let local_pos = Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    self.handle_left_press(state, local_pos, bounds.width)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &TrackListState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_function!();

        let mut frame = Frame::new(renderer, bounds.size());
        let canvas_w = bounds.size().width;
        let canvas_h = bounds.size().height;
        let palette = theme.extended_palette();
        let is_light = theme.is_light();

        frame.fill_rectangle(
            Point::new(0.0, 0.0),
            Size::new(canvas_w, canvas_h),
            palette.background.base.color,
        );

        let first = (self.scroll_y / self.track_height).floor() as usize;
        let visible_count = (canvas_h / self.track_height).ceil() as usize + 2;
        let last = (first + visible_count).min(self.tracks.len());

        for idx in first..last {
            let Some((track_id, name)) = self.tracks.get(idx) else {
                continue;
            };

            let track_y = idx as f32 * self.track_height - self.scroll_y;
            if track_y + self.track_height < 0.0 || track_y > canvas_h {
                continue;
            }

            let is_selected =
                state.selected_tracks.contains(track_id) || *track_id == self.selected_track;

            let track_color = self.track_colors.get(idx).copied().flatten();

            let bg_color = match track_color {
                Some(c) => {
                    if is_selected {
                        blend_color(c, palette.primary.weak.color, 0.35)
                    } else {
                        c
                    }
                }
                None => {
                    if is_selected {
                        palette.primary.weak.color
                    } else if idx % 2 == 0 {
                        if is_light {
                            Color::from_rgb(0.97, 0.97, 0.97)
                        } else {
                            Color::from_rgb(0.20, 0.20, 0.20)
                        }
                    } else if is_light {
                        Color::from_rgb(0.94, 0.94, 0.94)
                    } else {
                        Color::from_rgb(0.15, 0.15, 0.15)
                    }
                }
            };

            frame.fill_rectangle(
                Point::new(0.0, track_y),
                Size::new(canvas_w, self.track_height),
                bg_color,
            );

            // 未设置音轨颜色时，在左侧绘制默认小色块
            if track_color.is_none() {
                let badge_color = if is_light {
                    Color::from_rgb(0.6, 0.6, 0.6)
                } else {
                    Color::from_rgb(0.5, 0.5, 0.5)
                };
                frame.fill_rectangle(
                    Point::new(0.0, track_y),
                    Size::new(BADGE_WIDTH, self.track_height),
                    badge_color,
                );
            }

            let text_color = match track_color {
                Some(_) => contrast_text_color(bg_color),
                None => {
                    if is_selected {
                        palette.primary.strong.color
                    } else {
                        palette.background.base.text
                    }
                }
            };

            let text_x = BADGE_WIDTH + TEXT_MARGIN;
            let show_details = self.track_height >= 30.0;
            let track_num = format!("{:03}", track_id);

            if show_details {
                let small_size = (self.track_height * 0.25).clamp(8.0, 13.0);
                let label = self.track_labels.get(idx).cloned().unwrap_or_default();
                let label_text = if self.track_conductors.get(idx).copied().unwrap_or(false) {
                    "Master".to_string()
                } else if label.is_empty() {
                    let ch = self.track_channels.get(idx).copied().unwrap_or(0);
                    let port = (b'A' + (ch / 16).min(7)) as char;
                    format!("{}{:02}", port, (ch % 16) + 1)
                } else {
                    label
                };

                frame.fill_text(Text {
                    content: track_num,
                    position: Point::new(text_x, track_y + self.track_height * 0.30),
                    color: text_color,
                    size: iced_core::Pixels(small_size),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        small_size * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });

                frame.fill_text(Text {
                    content: label_text,
                    position: Point::new(text_x + 32.0, track_y + self.track_height * 0.30),
                    color: text_color,
                    size: iced_core::Pixels(small_size),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        small_size * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });

                let name_size = (self.track_height * 0.25).clamp(9.0, 13.0);
                frame.fill_text(Text {
                    content: name.clone(),
                    position: Point::new(text_x, track_y + self.track_height * 0.70),
                    color: text_color,
                    size: iced_core::Pixels(name_size),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        name_size * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });

                if !self.track_conductors.get(idx).copied().unwrap_or(false) {
                    let muted = state.track_muted.get(idx).copied().unwrap_or(false);
                    let soloed = state.track_soloed.get(idx).copied().unwrap_or(false);
                    let total_btn_w = 2.0 * BTN_SIZE + BTN_GAP;
                    let btn_x_start = canvas_w - total_btn_w - 6.0;
                    let btn_y = track_y + (self.track_height - BTN_SIZE) * 0.5;

                    let mute_fill = if muted {
                        palette.danger.base.color
                    } else if is_light {
                        Color::from_rgb(0.85, 0.85, 0.85)
                    } else {
                        Color::from_rgb(0.25, 0.25, 0.25)
                    };
                    frame.fill_rectangle(
                        Point::new(btn_x_start, btn_y),
                        Size::new(BTN_SIZE, BTN_SIZE),
                        mute_fill,
                    );
                    frame.fill_text(Text {
                        content: "M".to_string(),
                        position: Point::new(btn_x_start + BTN_SIZE * 0.5, btn_y + BTN_SIZE * 0.5),
                        color: if muted { Color::WHITE } else { text_color },
                        size: iced_core::Pixels(11.0),
                        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(13.0)),
                        font: iced_core::Font::default(),
                        max_width: f32::INFINITY,
                        align_x: iced_core::alignment::Horizontal::Center.into(),
                        align_y: iced_core::alignment::Vertical::Center,
                        shaping: iced_widget::text::Shaping::Advanced,
                    });

                    let solo_x = btn_x_start + BTN_SIZE + BTN_GAP;
                    let solo_fill = if soloed {
                        palette.warning.base.color
                    } else if is_light {
                        Color::from_rgb(0.85, 0.85, 0.85)
                    } else {
                        Color::from_rgb(0.25, 0.25, 0.25)
                    };
                    frame.fill_rectangle(
                        Point::new(solo_x, btn_y),
                        Size::new(BTN_SIZE, BTN_SIZE),
                        solo_fill,
                    );
                    frame.fill_text(Text {
                        content: "S".to_string(),
                        position: Point::new(solo_x + BTN_SIZE * 0.5, btn_y + BTN_SIZE * 0.5),
                        color: if soloed { Color::BLACK } else { text_color },
                        size: iced_core::Pixels(11.0),
                        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(13.0)),
                        font: iced_core::Font::default(),
                        max_width: f32::INFINITY,
                        align_x: iced_core::alignment::Horizontal::Center.into(),
                        align_y: iced_core::alignment::Vertical::Center,
                        shaping: iced_widget::text::Shaping::Advanced,
                    });
                }
            } else {
                let size = (self.track_height * 0.45).clamp(8.0, 14.0);
                frame.fill_text(Text {
                    content: track_num,
                    position: Point::new(text_x, track_y + self.track_height * 0.5),
                    color: text_color,
                    size: iced_core::Pixels(size),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        size * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });
                frame.fill_text(Text {
                    content: name.clone(),
                    position: Point::new(text_x + 40.0, track_y + self.track_height * 0.5),
                    color: text_color,
                    size: iced_core::Pixels(size),
                    line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(
                        size * 1.2,
                    )),
                    font: iced_core::Font::default(),
                    max_width: f32::INFINITY,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_widget::text::Shaping::Advanced,
                });
            }
        }

        let line_color = if is_light {
            Color::from_rgba(0.0, 0.0, 0.0, 0.08)
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.06)
        };
        let mut lb = iced_widget::canvas::path::Builder::new();
        lb.move_to(Point::new(canvas_w - 1.0, 0.0));
        lb.line_to(Point::new(canvas_w - 1.0, canvas_h));
        frame.stroke(
            &lb.build(),
            Stroke::default().with_color(line_color).with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}
