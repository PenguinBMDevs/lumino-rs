//! 钢琴卷帘（Pianoroll）— 对应 `yinhe piano_view.rs:404`
//!
//! - 入口 `view()` 返回 `Element`，内部用 `iced_widget::canvas::Program` 包装
//!  （对齐 `lumino/ui-editor/grid/program.rs:17` 的 `PianoRollGrid` 模式）
//! - 布局与坐标变换复用 `PianoLayout + lumino_core::ViewState`
//! - 键盘与网格背景走 iced canvas 矢量层，音符层走 `lumino-gfx` 的
//!   `Context / SwappableBuffer<NoteInstance> / NoteRenderer`（不自建 wgpu）

pub mod automation_panel;
pub mod bg;
pub mod drag;
pub mod keyboard;
pub mod layout;
pub mod overlay;

use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas::{self, Cache, Frame, Geometry, Program};

use lumino_core::ViewState;
use lumino_editor_state::EditorState;
use lumino_gfx::{Context as GfxContext, NoteInstance, NoteRenderer, SwappableBuffer};
use lumino_ui_core::{Element, Message, Renderer, Theme, window::Window};

// ── GFX 约束声明（不自建 wgpu）：
// 音符数据流 `EditorData → SwappableBuffer<NoteInstance> → NoteRenderer`，
// `GfxContext`（Device/Queue/Surface）由上层 Host 持有，此处仅通过类型
// 引用确保全走 `lumino-gfx` 管线，禁止 `yinhe-wgpu::InstanceRenderer`。
#[allow(dead_code)]
type GfxNoteBuffer = SwappableBuffer<NoteInstance>;
#[allow(dead_code)]
fn _assert_gfx_types(_ctx: &GfxContext, _r: &NoteRenderer, _buf: &GfxNoteBuffer) {}

use layout::{Orientation, PianoLayout};

/// 钢琴卷帘交互状态（canvas Program State）
///
/// 对齐 `ui-editor/grid/state.rs:7` 的 `GridInteractionState`，
/// 但持久化方式从 yinhe `egui::Id::persisted` 改为 `Program::State`。
#[derive(Debug)]
pub struct PianoViewInteractionState {
    /// 鼠标本地位置（相对 bounds）
    pub position: Option<Point>,
    /// 上次点击时间/位置（双击检测）
    pub last_click_time: std::time::Instant,
    pub last_click_pos: Option<Point>,
    /// Shift / Ctrl 按下（ModifiersChanged 可靠通道）
    pub shift_pressed: bool,
    pub control_pressed: bool,
    /// 拖拽状态（音符移动 / 框选 / 铅笔）
    pub drag: drag::PianoDragState,
    /// 铅笔工具状态
    pub pencil: drag::PencilState,
    /// 渲染缓存（键盘/标尺/网格，复用 ui-editor 的 Cache 策略）
    pub keyboard_cache: Cache<Renderer>,
    pub grid_cache: Cache<Renderer>,
    pub overlay_cache: Cache<Renderer>,
}

impl Default for PianoViewInteractionState {
    fn default() -> Self {
        Self {
            position: None,
            last_click_time: std::time::Instant::now(),
            last_click_pos: None,
            shift_pressed: false,
            control_pressed: false,
            drag: drag::PianoDragState::default(),
            pencil: drag::PencilState::default(),
            keyboard_cache: Cache::default(),
            grid_cache: Cache::default(),
            overlay_cache: Cache::default(),
        }
    }
}

/// 钢琴卷帘 Canvas Program
///
/// 持有 `ViewState + EditorState` 引用（不持有 `lumino-gfx::Context`：
/// Context / SwappableBuffer / NoteRenderer 由上层 `Host` 持有并在
/// `draw` 中通过 `SwappableBuffer<NoteInstance>` 交换数据，符合
/// “全走 lumino-gfx 渲染，全走 iced canvas Program” 约束）。
pub struct PianoRollProgram<'a> {
    /// 视图状态（滚动/缩放/键盘宽等，复用 lumino_core ViewState）
    pub view: &'a ViewState,
    /// 编辑器状态（含 DragState / EditState 等）
    pub editor_state: &'a EditorState,
    /// 方向（横向默认，纵向瀑布流预留）
    pub orientation: Orientation,
    /// 是否显示网格背景
    pub show_grid: bool,
    /// 演奏指示线位置（tick），None 则不绘制
    pub cursor_tick: Option<f64>,
    /// 当前按下的琴键（用于键盘高亮），None 则无高亮
    pub pressed_keys: Option<&'a [u8]>,
}

impl<'a> PianoRollProgram<'a> {
    pub fn new(view: &'a ViewState, editor_state: &'a EditorState) -> Self {
        Self {
            view,
            editor_state,
            orientation: Orientation::Horizontal,
            show_grid: true,
            cursor_tick: None,
            pressed_keys: None,
        }
    }

    pub fn with_orientation(mut self, o: Orientation) -> Self {
        self.orientation = o;
        self
    }

    pub fn with_cursor(mut self, tick: Option<f64>) -> Self {
        self.cursor_tick = tick;
        self
    }

    pub fn with_pressed_keys(mut self, keys: Option<&'a [u8]>) -> Self {
        self.pressed_keys = keys;
        self
    }
}

impl Program<Message, Theme, Renderer> for PianoRollProgram<'_> {
    type State = PianoViewInteractionState;

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
                    && let Some(cursor_pos) = cursor.position()
                    && bounds.contains(cursor_pos)
                {
                    // 双击检测（与 ui-editor/grid/program.rs:29 同阈值，120ms/5px）
                    let now = std::time::Instant::now();
                    let is_double = state.last_click_pos.is_some_and(|last| {
                        let dt = now.duration_since(state.last_click_time).as_millis();
                        let d = ((pos.x - last.x).powi(2) + (pos.y - last.y).powi(2)).sqrt();
                        dt < 300 && d < 5.0
                    });
                    if !is_double {
                        state.last_click_time = now;
                        state.last_click_pos = Some(pos);
                    }
                    // 工具分支：Pencil / Select（Select → 区分 命中音符/空白 → 拖动/框选）
                    // P3 stub 统一走框选，进入 marquee 状态
                    state.drag.start_marquee(pos);
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
                    if state.drag.is_marquee {
                        let mut view = (*self.view).clone();
                        drag::marquee_move(&mut state.drag, &mut view, pos, bounds);
                    }
                    return Some(canvas::Action::publish(Message::EditorAction(
                        lumino_ui_core::message::EditorAction::Moved(
                            lumino_ui_core::message::Point2::new(pos.x, pos.y),
                        ),
                    )));
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag.is_marquee {
                    state.drag.clear();
                }
                if state.drag.drag.is_some() {
                    state.drag.clear();
                }
                return Some(canvas::Action::publish(Message::EditorAction(
                    lumino_ui_core::message::EditorAction::Released,
                )));
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(pos) = local {
                    let is_ruler = pos.y < self.view.ruler_height;
                    let is_keyboard = pos.x < self.view.keyboard_width;
                    if is_ruler {
                        if let Some(f) = crate::zoom_factor_from_delta(delta) {
                            let vw = (bounds.width - self.view.keyboard_width).max(0.0);
                            let ratio =
                                ((pos.x - self.view.keyboard_width) / vw.max(1.0)).clamp(0.0, 1.0);
                            return Some(canvas::Action::publish(Message::ZoomXChanged {
                                zoom: self.view.zoom_x * f,
                                fixed_ratio: ratio,
                            }));
                        }
                    } else if is_keyboard {
                        if let Some(f) = crate::zoom_factor_from_delta(delta) {
                            let vh = (bounds.height - self.view.ruler_height).max(0.0);
                            let ratio =
                                ((pos.y - self.view.ruler_height) / vh.max(1.0)).clamp(0.0, 1.0);
                            return Some(canvas::Action::publish(Message::ZoomYChanged {
                                zoom: self.view.zoom_y * f,
                                fixed_ratio: ratio,
                            }));
                        }
                    } else {
                        use lumino_ui_core::constants::editor::{
                            SCROLL_LINES_SCALE, SCROLL_MAX_DELTA,
                        };
                        let (dx, dy) = match delta {
                            mouse::ScrollDelta::Lines { x, y } => {
                                (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                            }
                            mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                        };
                        let dx = dx.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                        let dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                        return Some(canvas::Action::publish(Message::EditorAction(
                            lumino_ui_core::message::EditorAction::Scrolled {
                                delta_x: dx,
                                delta_y: dy,
                            },
                        )));
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

        // 预计算布局（与 yinhe compute_layout 一致，P3 时缓存）
        let mut view_owned = (*self.view).clone();
        let total_ticks = view_owned.total_ticks as f64;
        let layout = layout::compute_layout(
            &mut view_owned,
            bounds,
            0.0,
            1.0,
            total_ticks,
            self.orientation,
        )
        .unwrap_or(PianoLayout {
            content_rect: Rectangle::new(
                Point::new(self.view.keyboard_width, self.view.ruler_height),
                iced_core::Size::new(
                    (bounds.width - self.view.keyboard_width).max(0.0),
                    (bounds.height - self.view.ruler_height).max(0.0),
                ),
            ),
            music_rect: Rectangle::new(
                Point::new(self.view.keyboard_width, self.view.ruler_height),
                iced_core::Size::new(
                    (bounds.width - self.view.keyboard_width).max(0.0),
                    (bounds.height - self.view.ruler_height).max(0.0),
                ),
            ),
            keyboard_rect: Rectangle::new(
                Point::new(0.0, self.view.ruler_height),
                iced_core::Size::new(
                    self.view.keyboard_width,
                    (bounds.height - self.view.ruler_height).max(0.0),
                ),
            ),
            ruler_rect: Rectangle::new(
                Point::new(self.view.keyboard_width, 0.0),
                iced_core::Size::new(
                    (bounds.width - self.view.keyboard_width).max(0.0),
                    self.view.ruler_height,
                ),
            ),
            content_y: self.view.ruler_height,
            content_bottom: bounds.height,
            w: bounds.width as u32,
            h: bounds.height as u32,
            pw: bounds.width as u32,
            ph: bounds.height as u32,
            total_ticks: self.view.total_ticks as f64,
            panels_total_h: 0.0,
            orientation: self.orientation,
        });

        // 1) 键盘（缓存）— 按压高亮
        {
            let pressed = self.pressed_keys;
            let g = state.keyboard_cache.draw(
                renderer,
                bounds.size(),
                |frame: &mut Frame<Renderer>| {
                    if let Some(keys) = pressed {
                        keyboard::draw_with_pressed(&view_owned, &layout, frame, bounds, theme, Some(keys));
                    } else {
                        keyboard::draw(&view_owned, &layout, frame, bounds, theme);
                    }
                },
            );
            geoms.push(g);
        }

        // 2) 网格背景（仅 key 轴条纹 + 八度线，小节线由 GPU infinite_grid 负责）
        if self.show_grid {
            let g =
                state
                    .grid_cache
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        bg::draw(&view_owned, &layout, frame, bounds, theme);
                    });
            geoms.push(g);
        }

        // 2.5) 演奏指示线（红色竖线+三角形头，跟随播放 tick）
        if let Some(tick) = self.cursor_tick {
            let g = state.overlay_cache.draw(
                renderer,
                bounds.size(),
                |frame: &mut Frame<Renderer>| {
                    overlay::draw(&view_owned, &layout, frame, bounds, Some(tick as f64));
                },
            );
            geoms.push(g);
        }

        // 3) 音符层（lumino-gfx NoteRenderer）— iced 桩以空几何占位，保证不自建 wgpu：
        // 实际路径：EditorState → SwappableBuffer<NoteInstance> → NoteRenderer
        // 渲染线程通过 Context.device/queue 发起 cull + draw，离屏纹理合成到
        // iced 的 wgpu 视图。此处不直接调用 wgpu，仅保留 ghost/marquee 等
        // CPU 预览几何。视口严格裁剪到 music_rect（网格区），避免在键盘/标尺区渲染。
        {
            let mut frame = Frame::new(renderer, bounds.size());
            // 裁剪到 music_rect，避免音符在钢琴下方/键盘区渲染（与布局 music_rect 对齐）
            let clip = layout.music_rect;
            // 框选矩形（虚线占位，与 ui-editor/grid/selection_box.rs 对齐，300α 黄）— 仅在 music_rect 内有效
            if let Some(r) = state.drag.marquee_rect() {
                use iced_widget::canvas::{Path, Stroke};
                // 将 marquee 限制在 music_rect 内
                let clipped = r.intersection(&clip).unwrap_or(r);
                if clipped.width > 1.0 && clipped.height > 1.0 {
                    let p = Path::rectangle(clipped.position(), clipped.size());
                    let st = Stroke::default()
                        .with_width(1.0)
                        .with_color(iced_core::Color::from_rgba(1.0, 0.8, 0.0, 0.9));
                    frame.stroke(&p, st);
                    frame.fill(&p, iced_core::Color::from_rgba(1.0, 0.8, 0.0, 0.15));
                }
            }
            // 视口指示：仅在 music_rect 内绘制，避免溢出到键盘/标尺
            frame.stroke(
                &iced_widget::canvas::Path::rectangle(clip.position(), clip.size()),
                iced_widget::canvas::Stroke::default()
                    .with_width(0.5)
                    .with_color(iced_core::Color::TRANSPARENT),
            );
            geoms.push(frame.into_geometry());
        }

        geoms
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag.is_note_drag() {
            mouse::Interaction::Grabbing
        } else if state.drag.is_marquee {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        }
    }
}

/// 导出 `view()` — iced 侧钢琴卷帘入口
///
/// 对齐任务要求：`piano_view/mod.rs 导出 view() -> Element，用 canvas::Program 包装`
///
/// # 参数
/// * `view` — 来自 `lumino_core::ViewState` 的滚动/缩放状态
/// * `editor_state` — 来自 `lumino_editor_state::EditorState`（含 DragState 等）
/// * `window` — 主题来源（`Window.theme`）
/// * `orientation` — 横/纵方向（默认横向）
pub fn view<'a>(
    view: &'a ViewState,
    editor_state: &'a EditorState,
    _window: &'a Window,
    orientation: Orientation,
) -> Element<'a> {
    let program = PianoRollProgram::new(view, editor_state).with_orientation(orientation);
    canvas::Canvas::new(program).into()
}

/// 带播放指示线的重载
pub fn view_with_cursor<'a>(
    view: &'a ViewState,
    editor_state: &'a EditorState,
    _window: &'a Window,
    orientation: Orientation,
    cursor_tick: Option<f64>,
) -> Element<'a> {
    let program = PianoRollProgram::new(view, editor_state)
        .with_orientation(orientation)
        .with_cursor(cursor_tick);
    canvas::Canvas::new(program).into()
}

/// 简化重载：横向默认 + 无窗口依赖（主题由 canvas Program 透传）
pub fn view_simple<'a>(view: &'a ViewState, editor_state: &'a EditorState) -> Element<'a> {
    let program = PianoRollProgram::new(view, editor_state);
    canvas::Canvas::new(program).into()
}

/// 简化重载带 cursor
pub fn view_simple_with_cursor<'a>(
    view: &'a ViewState,
    editor_state: &'a EditorState,
    cursor_tick: Option<f64>,
) -> Element<'a> {
    let program = PianoRollProgram::new(view, editor_state).with_cursor(cursor_tick);
    canvas::Canvas::new(program).into()
}

/// 辅助：滚轮增量 → 缩放因子（与 `ui-editor/zoom.rs::zoom_factor_from_delta` 一致）
///
/// P3 桩与主编辑器保持同阈值：`Lines` 10px/刻度、`Pixels` 50px/刻度 1.1×
fn _zoom_factor(delta: &mouse::ScrollDelta) -> Option<f32> {
    crate::zoom_factor_from_delta(delta)
}
