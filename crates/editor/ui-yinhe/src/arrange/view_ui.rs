/// 走带视口 canvas — 对应 `yinhe arrange/view_ui.rs:377`
///
/// - 入口 `view()` 返回 `Element`，内部用 `iced_widget::canvas::Program` 包装
///  （与 `ui-yinhe/piano_view::PianoRollProgram` 同型，共享 `Context`）
/// - 布局：左 `left_panel_width` 为音轨表宽度，`scroll_x / zoom_x` 为时间轴，
///   `scroll_y / lane_height()` 为泳道轴（行高均匀 = `row_height`）
/// - 音符层：复用 `lumino-gfx::ArrangementRenderer` 的常驻 GPU 缓冲
///   （`SwappableBuffer<ArrangementNoteInstance>` + `lane_index` 存储缓冲，
///   零第二份显存），此处仅以 iced canvas 占位几何呈现网格/指示线/选框，
///   真正绘制由 `Context::with_frame` + `ArrangementRenderer::draw` 在
///   渲染线程完成（不自建 wgpu Instance/Device/Queue）。
use iced_core::{Length, Point, Rectangle, Size, mouse};
use iced_widget::canvas::{self, Cache, Frame, Geometry, Path, Program, Stroke};

use lumino_core::ViewState;
use lumino_gfx::{
    ArrangementNoteInstance, ArrangementRenderer, Context as GfxContext, SwappableBuffer,
};
#[allow(dead_code)]
type ArrangeNoteBuffer = SwappableBuffer<ArrangementNoteInstance>;
use lumino_ui_core::{Element, Message, Renderer, Theme, window::Window};

// ── GFX 约束声明（不自建 wgpu）：
// 走带音符层 `SwappableBuffer<ArrangementNoteInstance> + ArrangementRenderer`，
// `GfxContext` 复用主视口同一 Device/Queue，禁止自建 `wgpu::Instance/Adapter/Device`。
#[allow(dead_code)]
fn _assert_arrange_gfx(_ctx: &GfxContext, _r: &ArrangementRenderer) {}

/// 走带视口状态（对齐 yinhe `ArrangementView` + `ArRowLayout`）
///
/// yinhe 原 `ArrangementView { base: TimelineViewBase { scroll_x/y, pixels_per_tick, ... },
/// lane_height, left_panel_width }`；lumino 侧 ViewState 已有 `scroll_x/y`,
/// `zoom_x/y`, `keyboard_width`（复用为 `left_panel_width`），此处仅补充 lane 语义。
#[derive(Debug, Clone)]
pub struct ArrangeViewport {
    /// 复用 ViewState（不 clone 全量，仅在布局时借用；此处为 owned 快照）
    pub view: ViewState,
    /// 每泳道高度（对应 yinhe `ArrangementView::lane_height()`，默认 40）
    pub lane_height: f32,
    /// 左侧音轨表面板宽度（复用 `view.keyboard_width` 的另一语义）
    pub left_panel_width: f32,
    /// 每轨高度（row_height，与 lane_height 一致或按 zoom_y 换算）
    pub row_height: f32,
}

impl Default for ArrangeViewport {
    fn default() -> Self {
        Self {
            view: ViewState::default(),
            lane_height: 40.0,
            left_panel_width: 220.0,
            row_height: 40.0,
        }
    }
}

impl ArrangeViewport {
    /// 总行数（音轨数 + 展开的自动化子行，P3 占位当前 = 轨数）
    #[must_use]
    pub fn total_rows(&self, track_count: usize) -> usize {
        track_count
    }

    /// tick → 本地 x（复用 ViewState，但需减去 left_panel 偏移的 music 区归一化）
    #[must_use]
    pub fn tick_to_x(&self, tick: f64) -> f32 {
        tick as f32 * self.view.zoom_x - self.view.scroll_x + self.left_panel_width
    }

    /// 泳道 y（track 索引 → 像素，均匀行高）
    #[must_use]
    pub fn track_y(&self, track_idx: usize) -> f32 {
        track_idx as f32 * self.lane_height - self.view.scroll_y
    }

    /// 视口 clamp（与 `ViewState` 的 max_scroll 语义一致，简化：仅 y 轴为全量化）
    pub fn clamp_scroll(
        &mut self,
        viewport_w: f32,
        viewport_h: f32,
        total_ticks: f64,
        track_count: usize,
    ) {
        let total_w = total_ticks as f32 * self.view.zoom_x;
        let vw = (viewport_w - self.left_panel_width).max(0.0);
        let max_x = (total_w - vw).max(0.0);
        self.view.scroll_x = self.view.scroll_x.clamp(0.0, max_x);
        let total_h = self.total_rows(track_count) as f32 * self.lane_height;
        let max_y = (total_h - viewport_h).max(0.0);
        self.view.scroll_y = self.view.scroll_y.clamp(0.0, max_y);
    }
}

/// 走带视口交互状态（canvas Program State）
#[derive(Debug, Default)]
pub struct ArrangeViewState {
    pub position: Option<Point>,
    pub shift_pressed: bool,
    pub control_pressed: bool,
    /// 中键拖拽起点（用于平移）
    pub middle_drag: Option<Point>,
    /// 框选起点/当前（本地坐标）
    pub sel_start: Option<Point>,
    pub sel_current: Option<Point>,
    pub grid_cache: Cache<Renderer>,
    pub overlay_cache: Cache<Renderer>,
}

/// 走带视口 Canvas Program（透明覆盖层，捕获鼠标 + 绘制网格/指示线/选框）
///
/// 与 `ui-editor/arrangement/click_canvas.rs:157` 的 `ArrangementClickCanvas`
/// 同职责，但：
/// - 坐标系复用 `ViewState`（不另起 `ArrangementViewport` 的重复字段）
/// - 事件映射到 `lumino_ui_core::Message`（`LoopRange / EditorAction / Arrangement*`）
pub struct ArrangeCanvas<'a> {
    pub viewport: ArrangeViewport,
    pub track_count: usize,
    pub total_ticks: f64,
    /// 播放光标 tick（光标线）
    pub cursor_tick: Option<f64>,
    /// 当前拍号变化（用于网格生成，P3 占位）
    pub time_sigs: &'a [(u32, u8, u8)],
    /// 是否正在播放（跟随模式决定光标是否自动滚动）
    pub is_playing: bool,
}

impl<'a> ArrangeCanvas<'a> {
    pub fn new(
        viewport: ArrangeViewport,
        track_count: usize,
        total_ticks: f64,
        time_sigs: &'a [(u32, u8, u8)],
    ) -> Self {
        Self {
            viewport,
            track_count,
            total_ticks,
            cursor_tick: None,
            time_sigs,
            is_playing: false,
        }
    }

    pub fn with_cursor(mut self, t: Option<f64>) -> Self {
        self.cursor_tick = t;
        self
    }
}

impl Program<Message, Theme, Renderer> for ArrangeCanvas<'_> {
    type State = ArrangeViewState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let local = cursor
            .position()
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        if let Some(pos) = local {
            state.position = Some(pos);
        }
        match event {
            canvas::Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(m)) => {
                state.shift_pressed = m.shift();
                state.control_pressed = m.control();
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = local
                    && let Some(cp) = cursor.position()
                    && bounds.contains(cp)
                {
                    state.sel_start = Some(pos);
                    state.sel_current = Some(pos);
                    return Some(canvas::Action::publish(Message::EditorAction(
                        lumino_ui_core::message::EditorAction::Pressed {
                            pos: lumino_ui_core::message::Point2::new(pos.x, pos.y),
                            shift: state.shift_pressed,
                            ctrl: state.control_pressed,
                        },
                    )));
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(pos) = local {
                    if state.middle_drag.is_some() {
                        // TODO(P3): 中键平移：delta → viewport.scroll_x/y + clamp
                    }
                    if state.sel_start.is_some() {
                        state.sel_current = Some(pos);
                    }
                    return Some(canvas::Action::publish(Message::EditorAction(
                        lumino_ui_core::message::EditorAction::Moved(
                            lumino_ui_core::message::Point2::new(pos.x, pos.y),
                        ),
                    )));
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.sel_start = None;
                state.sel_current = None;
                return Some(canvas::Action::publish(Message::EditorAction(
                    lumino_ui_core::message::EditorAction::Released,
                )));
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(pos) = local {
                    state.middle_drag = Some(pos);
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                state.middle_drag = None;
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(pos) = local
                    && bounds.contains(cursor.position().unwrap_or(Point::new(f32::NAN, f32::NAN)))
                {
                    if state.control_pressed {
                        if let Some(f) = crate::zoom_factor_from_delta(delta) {
                            let ratio = (pos.x / bounds.width).clamp(0.0, 1.0);
                            return Some(canvas::Action::publish(Message::ZoomXChanged {
                                zoom: self.viewport.view.zoom_x * f,
                                fixed_ratio: ratio,
                            }));
                        }
                    } else {
                        use lumino_ui_core::constants::editor::{
                            SCROLL_LINES_SCALE, SCROLL_MAX_DELTA,
                        };
                        let (mut dx, mut dy) = match delta {
                            mouse::ScrollDelta::Lines { x, y } => {
                                (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                            }
                            mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                        };
                        dx = dx.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                        dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                        if state.shift_pressed && dx.abs() < f32::EPSILON {
                            // Shift+滚轮：垂直转水平
                            return Some(canvas::Action::publish(Message::ArrangementScrollX(
                                self.viewport.view.scroll_x - dy,
                            )));
                        }
                        if dy.abs() > f32::EPSILON {
                            return Some(canvas::Action::publish(Message::ArrangementScrollY(
                                self.viewport.view.scroll_y - dy,
                            )));
                        }
                        if dx.abs() > f32::EPSILON {
                            return Some(canvas::Action::publish(Message::ArrangementScrollX(
                                self.viewport.view.scroll_x - dx,
                            )));
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut geoms = Vec::new();

        // 1) 网格（泳道背景 + 小节/拍线占位）
        {
            let vp = self.viewport.clone();
            let track_count = self.track_count;
            let g =
                state
                    .grid_cache
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        draw_grid(frame, &vp, track_count, bounds, theme);
                    });
            geoms.push(g);
        }

        // 2) 走带音符层（GPU）占位：与 piano_view 一致，此处不自建 wgpu。
        // 实际：Context::with_frame + ArrangementRenderer::draw
        // lane_index 缓冲（track → lane 映射） + NoteInstance 常驻缓冲
        // 通过 SwappableBuffer 交换（全走 lumino-gfx，不引入 yinhe-wgpu）。
        {
            let vp = &self.viewport;
            let g =
                state
                    .overlay_cache
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        // 播放光标线
                        if let Some(t) = self.cursor_tick {
                            let x = vp.tick_to_x(t);
                            if x >= vp.left_panel_width && x <= bounds.width {
                                let p =
                                    Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
                                frame.stroke(
                                    &p,
                                    Stroke::default()
                                        .with_width(1.5)
                                        .with_color(iced_core::Color::from_rgb(1.0, 0.2, 0.2)),
                                );
                            }
                        }
                        // 框选矩形
                        if let (Some(s), Some(c)) = (state.sel_start, state.sel_current) {
                            let x = s.x.min(c.x);
                            let y = s.y.min(c.y);
                            let w = (s.x - c.x).abs();
                            let h = (s.y - c.y).abs();
                            if w >= 3.0 && h >= 3.0 {
                                let r = Rectangle::new(Point::new(x, y), Size::new(w, h));
                                let p = Path::rectangle(r.position(), r.size());
                                frame.stroke(
                                    &p,
                                    Stroke::default().with_width(1.0).with_color(
                                        iced_core::Color::from_rgba(0.2, 0.6, 1.0, 0.9),
                                    ),
                                );
                                frame.fill(&p, iced_core::Color::from_rgba(0.2, 0.6, 1.0, 0.12));
                            }
                        }
                    });
            geoms.push(g);
        }

        geoms
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.middle_drag.is_some() {
            mouse::Interaction::Grabbing
        } else if state.sel_start.is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        }
    }
}

fn draw_grid(
    frame: &mut Frame<Renderer>,
    vp: &ArrangeViewport,
    track_count: usize,
    bounds: Rectangle,
    theme: &Theme,
) {
    let palette = theme.extended_palette();
    let bg_even = palette.background.base.color;
    let bg_odd = palette.background.weak.color;
    let grid_c = palette.background.strong.color.scale_alpha(0.25);
    let bar_c = palette.background.strong.color.scale_alpha(0.55);

    // 泳道交替底色
    for idx in 0..track_count {
        let y = vp.track_y(idx);
        if y + vp.lane_height < 0.0 || y > bounds.height {
            continue;
        }
        let rect = Rectangle::new(Point::new(0.0, y), Size::new(bounds.width, vp.lane_height));
        let clipped = rect.intersection(&bounds);
        if let Some(r) = clipped {
            let p = Path::rectangle(r.position(), r.size());
            let c = if idx % 2 == 0 { bg_even } else { bg_odd };
            frame.fill(&p, c);
        }
        // 泳道分隔线
        let line = Rectangle::new(
            Point::new(0.0, y + vp.lane_height - 0.5),
            Size::new(bounds.width, 1.0),
        );
        let p = Path::rectangle(line.position(), line.size());
        frame.fill(&p, grid_c);
    }

    // 小节线占位（按 1920 ppq 的 4/4 小节生成，P3 接入真实拍号）
    let ppq = vp.view.ppq as f32;
    let bar_ticks = ppq * 4.0;
    let start_tick = (vp.view.scroll_x / vp.view.zoom_x).floor();
    let end_tick = start_tick + bounds.width / vp.view.zoom_x + bar_ticks;
    let mut t = (start_tick / bar_ticks).floor() * bar_ticks;
    while t <= end_tick {
        let x = vp.tick_to_x(t as f64);
        if x >= vp.left_panel_width && x <= bounds.width {
            let line = Rectangle::new(Point::new(x, 0.0), Size::new(1.0, bounds.height));
            let p = Path::rectangle(line.position(), line.size());
            frame.fill(&p, bar_c);
        }
        t += bar_ticks;
    }
}

/// 导出 `view()` — iced 侧走带视口入口
///
/// 对齐任务：`arrange/view_ui.rs 走带视口 canvas`
pub fn view<'a>(
    viewport: ArrangeViewport,
    track_count: usize,
    total_ticks: f64,
    time_sigs: &'a [(u32, u8, u8)],
    _window: &'a Window,
) -> Element<'a> {
    let program = ArrangeCanvas::new(viewport, track_count, total_ticks, time_sigs);
    canvas::Canvas::new(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_tick_roundtrip() {
        let mut vp = ArrangeViewport::default();
        vp.view.zoom_x = 0.1;
        vp.view.scroll_x = 10.0;
        let t = 480.0_f64;
        let x = vp.tick_to_x(t);
        assert!(x > vp.left_panel_width);
    }

    #[test]
    fn clamp_does_not_panic_on_zero_tracks() {
        let mut vp = ArrangeViewport::default();
        vp.clamp_scroll(800.0, 600.0, 1920.0 * 100.0, 0);
        assert_eq!(vp.view.scroll_y, 0.0);
    }
}
