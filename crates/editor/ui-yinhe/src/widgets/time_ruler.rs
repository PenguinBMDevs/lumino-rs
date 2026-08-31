//! 时间标尺 — yinhe `widgets/time_ruler.rs:608` 的 iced 迁移
//!
//! 小节/拍号刻度，可点击跳转，跟随 `ViewState` 的 `scroll_x / zoom_x`。
//! 密度自适应（measure / beat / sub / tick），支持变拍号段，支持横/纵方向。

use iced_core::mouse::{self, Cursor};
use iced_core::{Length, Point, Rectangle, Size};
use iced_widget::canvas::{self, Cache, Frame, Geometry, Program, Text};

use lumino_ui_core::{Message as LuminoMessage, Renderer, Theme};

const MIN_LABEL_SPACING: f32 = 38.0;
const SUB_BEAT_DIV: u32 = 4;

/// 拍号段（对齐 yinhe `build_time_sig_segments` 的简化版）
#[derive(Debug, Clone, Copy)]
pub struct TimeSigSegment {
    pub start_tick: u32,
    pub num: u8,
    pub den: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulerOrientation {
    Horizontal,
    Vertical,
}

fn measure_ticks(tpb: u32, num: u8, den: u8) -> u32 {
    let den = den.max(1) as u32;
    let num = num.max(1) as u32;
    let beat_ticks = tpb * 4 / den;
    beat_ticks * num
}

fn build_segments(
    events: &[TimeSigSegment],
    default_num: u8,
    default_den: u8,
) -> Vec<(u32, u8, u8)> {
    let mut segs = Vec::new();
    if events.is_empty() {
        segs.push((0, default_num.max(1), default_den.max(1)));
        return segs;
    }
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|s| s.start_tick);
    // ensure starts at 0
    if sorted[0].start_tick != 0 {
        segs.push((0, default_num.max(1), default_den.max(1)));
    }
    for s in sorted {
        if let Some(last) = segs.last_mut() {
            if last.0 == s.start_tick {
                *last = (s.start_tick, s.num.max(1), s.den.max(1));
                continue;
            }
        }
        segs.push((s.start_tick, s.num.max(1), s.den.max(1)));
    }
    segs
}

fn cumulative_bar_offsets(tpb: u32, segments: &[(u32, u8, u8)]) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(segments.len());
    let mut acc = 0u32;
    for i in 0..segments.len() {
        offsets.push(acc);
        if i + 1 < segments.len() {
            let (start, num, den) = segments[i];
            let end = segments[i + 1].0;
            let tm = measure_ticks(tpb, num, den);
            if tm > 0 && end > start {
                acc += (end - start) / tm;
            }
        }
    }
    offsets
}

fn compute_measure_divisor(pixels_per_measure: f32, min_spacing: f32) -> u32 {
    let mut divisor = 1u32;
    while pixels_per_measure * (divisor as f32) < min_spacing && divisor < 64 {
        divisor *= 2;
    }
    divisor
}

/// 时间标尺状态（跟踪拖动）
#[derive(Default)]
pub struct TimeRulerState {
    dragging: bool,
    cache: Cache<Renderer>,
}

/// 时间标尺 Canvas Program
pub struct TimeRuler<'a> {
    pub tpb: u32,
    pub pixels_per_tick: f32,
    pub scroll: f32,
    pub left_panel_width: f32,
    pub segments: &'a [TimeSigSegment],
    pub default_num: u8,
    pub default_den: u8,
    pub orientation: RulerOrientation,
    pub theme: &'a Theme,
}

impl<'a> TimeRuler<'a> {
    fn tick_to_main_px(&self, tick: f64) -> f32 {
        match self.orientation {
            RulerOrientation::Horizontal => tick as f32 * self.pixels_per_tick - self.scroll,
            RulerOrientation::Vertical => tick as f32 * self.pixels_per_tick - self.scroll,
        }
    }

    fn main_px_to_tick(&self, px: f32) -> f64 {
        ((px + self.scroll) / self.pixels_per_tick.max(0.001)) as f64
    }

    fn publish_scrub(&self, tick: f64) -> canvas::Action<LuminoMessage> {
        let snapped = (tick / (self.tpb as f64 / 4.0)).round() * (self.tpb as f64 / 4.0);
        let snapped_f = snapped.max(0.0) as f32;
        let scrub =
            LuminoMessage::EditorAction(lumino_message::EditorAction::Scrubbed { tick: snapped_f });
        let cursor_msg = LuminoMessage::ArrangementCursorSet(snapped);
        canvas::Action::publish(LuminoMessage::Batch(vec![scrub, cursor_msg]))
    }
}

impl<'a> Program<LuminoMessage, Theme, Renderer> for TimeRuler<'a> {
    type State = TimeRulerState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        // 背景
        let bg = if lumino_ui_core::theme::is_high_contrast() {
            lumino_ui_core::theme::hc::RULER_BG
        } else if is_light(theme) {
            palette.background.weakest.color
        } else {
            palette.background.base.color
        };
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);
        // 底部分割线
        let border = border_color(theme);
        let line_y = bounds.height - 0.5;
        frame.fill_rectangle(Point::new(0.0, line_y), Size::new(bounds.width, 1.0), border.scale_alpha(0.5));

        if self.pixels_per_tick <= 0.001 {
            return vec![frame.into_geometry()];
        }

        let tpb = self.tpb.max(1);
        let ppu = self.pixels_per_tick;

        let main_size = match self.orientation {
            RulerOrientation::Horizontal => bounds.width,
            RulerOrientation::Vertical => bounds.height,
        };
        let tick_start = self.main_px_to_tick(0.0).max(0.0);
        let tick_end = self.main_px_to_tick(main_size);

        let text_cross_center = match self.orientation {
            RulerOrientation::Horizontal => bounds.height / 2.0,
            RulerOrientation::Vertical => bounds.width / 2.0,
        };

        let ticks_per_sub = (tpb / SUB_BEAT_DIV).max(1);
        let segs_raw = build_segments(self.segments, self.default_num.max(1), self.default_den.max(1));
        let bar_offsets = cumulative_bar_offsets(tpb, &segs_raw);

        let pixels_per_beat = tpb as f32 * ppu;
        let pixels_per_sub = ticks_per_sub as f32 * ppu;

        let show_beat = pixels_per_beat >= MIN_LABEL_SPACING;
        let show_sub = pixels_per_sub >= MIN_LABEL_SPACING;
        let show_tick = ppu >= MIN_LABEL_SPACING;

        let tick_step = if show_tick {
            (MIN_LABEL_SPACING / ppu).ceil() as u32
        } else {
            0
        };

        let font_size = 10.0;
        let measure_col = if lumino_ui_core::theme::is_high_contrast() {
            lumino_ui_core::theme::hc::BAR_LINE
        } else {
            palette.primary.strong.color
        };
        let beat_col = text_color(theme).scale_alpha(0.9);
        let sub_col = text_color(theme).scale_alpha(0.65);
        let tick_col = text_color(theme).scale_alpha(0.45);

        for i in 0..segs_raw.len() {
            let (seg_start, num, den) = segs_raw[i];
            let seg_end = segs_raw.get(i + 1).map_or(u32::MAX, |&(t, _, _)| t);
            let seg_start_f = seg_start as f64;
            if seg_start_f > tick_end {
                break;
            }
            let ticks_per_measure = measure_ticks(tpb, num, den);
            if ticks_per_measure == 0 {
                continue;
            }
            let ticks_per_beat = ticks_per_measure / num as u32;
            let bar_offset = bar_offsets[i];

            let pixels_per_measure = ticks_per_measure as f32 * ppu;
            let measure_divisor = compute_measure_divisor(pixels_per_measure, MIN_LABEL_SPACING);
            let merged_measure_ticks = ticks_per_measure.saturating_mul(measure_divisor);

            let main_step = if show_beat || show_sub {
                ticks_per_sub
            } else {
                merged_measure_ticks.max(1)
            };

            let first_tick_f = seg_start_f.max(tick_start);
            let step_f = main_step as f64;
            let first = seg_start.saturating_add(
                (((first_tick_f - seg_start_f) / step_f).floor() as u32).saturating_mul(main_step),
            );

            let mut tick = first;
            while (tick as f64) <= tick_end && tick < seg_end {
                let local = tick - seg_start;
                let main_px = self.tick_to_main_px(tick as f64);
                if main_px >= 0.0 && main_px <= main_size {
                    let is_measure = local % merged_measure_ticks == 0;
                    let is_beat = if !is_measure {
                        local % ticks_per_measure == 0 || (local % ticks_per_measure) % ticks_per_beat == 0
                    } else {
                        false
                    };
                    // Determine label and color
                    let (label_opt, color) = if is_measure {
                        let bar = bar_offset + (local / ticks_per_measure) + 1;
                        (Some(format!("{}", bar)), measure_col)
                    } else if is_beat && show_beat {
                        // beat label only if not measure
                        let beat_in_measure = (local % ticks_per_measure) / ticks_per_beat;
                        let is_actual_beat = (local % ticks_per_beat) == 0;
                        if is_actual_beat {
                            let bar = bar_offset + (local / ticks_per_measure) + 1;
                            let beat = beat_in_measure + 1;
                            (Some(format!("{}.{}", bar, beat)), beat_col)
                        } else {
                            (None, beat_col)
                        }
                    } else if show_sub {
                        if show_tick {
                            let bar = bar_offset + (local / ticks_per_measure) + 1;
                            let beat = (local % ticks_per_measure) / ticks_per_beat + 1;
                            let tick_in_beat = (tick as f64 % tpb as f64) as u32;
                            (
                                Some(format!("{}.{}.{:03}", bar, beat, tick_in_beat)),
                                tick_col,
                            )
                        } else {
                            // sub beat label
                            let is_sub = local % ticks_per_beat == 0 || local % ticks_per_sub == 0;
                            if is_sub {
                                let bar = bar_offset + (local / ticks_per_measure) + 1;
                                let beat = (local % ticks_per_measure) / ticks_per_beat + 1;
                                let sub = (local % ticks_per_beat) / ticks_per_sub;
                                (Some(format!("{}.{}.{}", bar, beat, sub)), sub_col)
                            } else {
                                (None, sub_col)
                            }
                        }
                    } else {
                        (None, sub_col)
                    };

                    if let Some(label) = label_opt {
                        // 刻度线
                        let line_width = if is_measure { 1.5 } else { 1.0 };
                        let line_alpha = if is_measure { 0.55 } else { 0.28 };
                        let line_color = if is_measure { measure_col.scale_alpha(line_alpha) } else { border.scale_alpha(line_alpha) };
                        match self.orientation {
                            RulerOrientation::Horizontal => {
                                let lr = Rectangle::new(
                                    Point::new(main_px - line_width / 2.0, 0.0),
                                    Size::new(line_width, bounds.height),
                                );
                                frame.fill_rectangle(lr.position(), lr.size(), line_color);
                                let pos = Point::new(main_px + 2.0, text_cross_center);
                                // 防止文字溢出 track
                                if main_px + MIN_LABEL_SPACING <= main_size + 30.0 {
                                    frame.fill_text(Text {
                                        content: label,
                                        position: pos,
                                        color,
                                        size: iced_core::Pixels(font_size),
                                        font: iced_core::Font::MONOSPACE,
                                        align_x: iced_core::alignment::Horizontal::Left.into(),
                                        align_y: iced_core::alignment::Vertical::Center,
                                        shaping: iced_core::text::Shaping::Basic,
                                        max_width: MIN_LABEL_SPACING * 2.0,
                                        line_height: iced_core::text::LineHeight::Relative(1.0),
                                    });
                                }
                            }
                            RulerOrientation::Vertical => {
                                let lr = Rectangle::new(
                                    Point::new(0.0, main_px - line_width / 2.0),
                                    Size::new(bounds.width, line_width),
                                );
                                frame.fill_rectangle(lr.position(), lr.size(), line_color);
                                let pos = Point::new(text_cross_center, main_px);
                                frame.fill_text(Text {
                                    content: label.clone(),
                                    position: pos,
                                    color,
                                    size: iced_core::Pixels(9.0),
                                    font: iced_core::Font::MONOSPACE,
                                    align_x: iced_core::alignment::Horizontal::Center.into(),
                                    align_y: iced_core::alignment::Vertical::Center,
                                    shaping: iced_core::text::Shaping::Basic,
                                    max_width: bounds.width,
                                    line_height: iced_core::text::LineHeight::Relative(1.0),
                                });
                            }
                        }
                    }
                }
                tick = tick.saturating_add(main_step);
                if main_step == 0 {
                    break;
                }
            }

            // fine tick loop
            if tick_step > 0 && tick_step < ticks_per_sub {
                let first_u = seg_start.max(tick_start as u32);
                let first_aligned = seg_start.saturating_add(
                    (first_u - seg_start).div_ceil(tick_step).saturating_mul(tick_step),
                );
                let mut ft = first_aligned;
                while (ft as f64) <= tick_end && ft < seg_end {
                    let local = ft - seg_start;
                    let is_measure = local % ticks_per_measure == 0;
                    let is_beat_line = (local % ticks_per_measure).is_multiple_of(ticks_per_beat);
                    let is_sub_line = local % ticks_per_sub == 0;
                    if !is_measure && !is_beat_line && !is_sub_line {
                        let main_px = self.tick_to_main_px(ft as f64);
                        if main_px >= 0.0 && main_px <= main_size {
                            let bar = bar_offset + (local / ticks_per_measure) + 1;
                            let beat = (local % ticks_per_measure) / ticks_per_beat + 1;
                            let tick_in_beat = (ft as f64 % tpb as f64) as u32;
                            let label = format!("{}.{}.{:03}", bar, beat, tick_in_beat);
                            match self.orientation {
                                RulerOrientation::Horizontal => {
                                    let lr = Rectangle::new(
                                        Point::new(main_px, 0.0),
                                        Size::new(0.5, bounds.height),
                                    );
                                    frame.fill_rectangle(lr.position(), lr.size(), border.scale_alpha(0.18));
                                    frame.fill_text(Text {
                                        content: label,
                                        position: Point::new(main_px + 2.0, text_cross_center),
                                        color: tick_col,
                                        size: iced_core::Pixels(9.0),
                                        font: iced_core::Font::MONOSPACE,
                                        align_x: iced_core::alignment::Horizontal::Left.into(),
                                        align_y: iced_core::alignment::Vertical::Center,
                                        shaping: iced_core::text::Shaping::Basic,
                                        max_width: MIN_LABEL_SPACING,
                                        line_height: iced_core::text::LineHeight::Relative(1.0),
                                    });
                                }
                                RulerOrientation::Vertical => {
                                    let lr = Rectangle::new(
                                        Point::new(0.0, main_px),
                                        Size::new(bounds.width, 0.5),
                                    );
                                    frame.fill_rectangle(lr.position(), lr.size(), border.scale_alpha(0.18));
                                }
                            }
                        }
                    }
                    ft = ft.saturating_add(tick_step);
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced_core::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<LuminoMessage>> {
        match event {
            iced_core::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position()?;
                if !bounds.contains(pos) {
                    return None;
                }
                let rel = match self.orientation {
                    RulerOrientation::Horizontal => pos.x - bounds.x,
                    RulerOrientation::Vertical => pos.y - bounds.y,
                };
                let tick = self.main_px_to_tick(rel).max(0.0);
                state.dragging = true;
                state.cache.clear();
                return Some(self.publish_scrub(tick));
            }
            iced_core::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                state.cache.clear();
                return Some(canvas::Action::request_redraw());
            }
            iced_core::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(pos) = cursor.position() {
                        // clamp to bounds for smooth drag outside
                        let rel = match self.orientation {
                            RulerOrientation::Horizontal => (pos.x - bounds.x).clamp(0.0, bounds.width),
                            RulerOrientation::Vertical => (pos.y - bounds.y).clamp(0.0, bounds.height),
                        };
                        let tick = self.main_px_to_tick(rel).max(0.0);
                        return Some(self.publish_scrub(tick));
                    }
                }
            }
            iced_core::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(pos) = cursor.position() {
                    if bounds.contains(pos) {
                        if let Some(f) = crate::zoom_factor_from_delta(delta) {
                            let rel = match self.orientation {
                                RulerOrientation::Horizontal => pos.x - bounds.x,
                                RulerOrientation::Vertical => pos.y - bounds.y,
                            };
                            let viewport = match self.orientation {
                                RulerOrientation::Horizontal => bounds.width,
                                RulerOrientation::Vertical => bounds.height,
                            };
                            let ratio = (rel / viewport.max(1.0)).clamp(0.0, 1.0);
                            return Some(canvas::Action::publish(LuminoMessage::ZoomXChanged {
                                zoom: self.pixels_per_tick * f,
                                fixed_ratio: ratio,
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if let Some(pos) = cursor.position() {
            if bounds.contains(pos) {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::None
    }
}

fn is_light(theme: &Theme) -> bool {
    if lumino_ui_core::theme::is_high_contrast() {
        return false;
    }
    theme.extended_palette().background.weakest.color.r > 0.5
}

fn border_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::BORDER;
    }
    let p = theme.extended_palette().background;
    if is_light(theme) {
        p.strongest.color
    } else {
        p.base.color
    }
}

fn text_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::TEXT;
    }
    if is_light(theme) {
        iced_core::Color::BLACK
    } else {
        iced_core::Color::WHITE
    }
}

/// 构建时间标尺 Element（横向，默认）
pub fn view<'a>(
    tpb: u32,
    pixels_per_tick: f32,
    scroll: f32,
    left_panel_width: f32,
    segments: &'a [TimeSigSegment],
    theme: &'a Theme,
) -> iced_core::Element<'a, LuminoMessage, Theme, Renderer> {
    view_with_orientation(tpb, pixels_per_tick, scroll, left_panel_width, segments, theme, RulerOrientation::Horizontal)
}

pub fn view_with_orientation<'a>(
    tpb: u32,
    pixels_per_tick: f32,
    scroll: f32,
    left_panel_width: f32,
    segments: &'a [TimeSigSegment],
    theme: &'a Theme,
    orientation: RulerOrientation,
) -> iced_core::Element<'a, LuminoMessage, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(TimeRuler {
        tpb: tpb.max(1),
        pixels_per_tick,
        scroll,
        left_panel_width,
        segments,
        default_num: 4,
        default_den: 4,
        orientation,
        theme,
    })
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .into()
}

/// 带拍号默认值的构造
pub fn view_with_time_sig<'a>(
    tpb: u32,
    pixels_per_tick: f32,
    scroll: f32,
    left_panel_width: f32,
    segments: &'a [TimeSigSegment],
    default_num: u8,
    default_den: u8,
    theme: &'a Theme,
) -> iced_core::Element<'a, LuminoMessage, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(TimeRuler {
        tpb: tpb.max(1),
        pixels_per_tick,
        scroll,
        left_panel_width,
        segments,
        default_num,
        default_den,
        orientation: RulerOrientation::Horizontal,
        theme,
    })
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .into()
}

/// 便捷：直接从 `ViewState` 构造时间标尺（联动 scroll_x / zoom_x）
pub fn view_for_view<'a>(
    view: &lumino_core::ViewState,
    segments: &'a [TimeSigSegment],
    theme: &'a Theme,
) -> iced_core::Element<'a, LuminoMessage, Theme, Renderer> {
    view_with_time_sig(
        view.ppq as u32,
        view.zoom_x,
        view.scroll_x,
        view.keyboard_width,
        segments,
        4,
        4,
        theme,
    )
}
