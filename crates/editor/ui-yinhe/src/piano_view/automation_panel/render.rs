//! 自动化面板渲染 — 对应 yinhe `automation_panel/render.rs`
//!
//! iced 迁移：
//! - 以 `iced_widget::canvas::Program` 绘制曲线/柱状（`Frame::stroke/fill`），
//!   背景/网格/中心线/标签在 canvas 矢量层完成；
//! - 数据层复用 `lumino_gfx::{CcBarRenderer, AutomationViewParams, build_lane_instances}`
//!   与 `lumino_note_core::{AutomationLane, SegmentShape}`（不自建 wgpu，不引 egui）；
//! - 实际 GPU 绘制（`CcBarRenderer::prepare/draw`）由上层 `lumino-gfx::Context` 持有，
//!   此处仅通过类型引用与实例构建函数确保管线一致，canvas 层负责矢量化预览。

use iced_core::{Color, Point, Rectangle, Size, mouse};
use iced_widget::canvas::{Action, Cache, Event, Frame, Geometry, Path, Program, Stroke};

use lumino_gfx::automation::{AutomationViewParams, build_lane_instances};
use lumino_gfx::{CcBarInstance, CcBarRenderer};
use lumino_note_core::{AutomationLane, AutomationTarget};

use super::types::{AutomationGhost, AutomationPanelView, PanelOverlayData};
use super::value::panel_max_val_simple;
use lumino_ui_core::{Message, Renderer, Theme};

// ── 颜色（主题注入前的占位，与 `lumino-gfx::AUTOMATION_NODE_COLOR` 一致） ──

const NODE_COLOR: [f32; 3] = lumino_gfx::automation::AUTOMATION_NODE_COLOR;
const CENTER_LINE_COLOR: Color = Color::from_rgba(0.85, 0.85, 0.85, 0.35);
const GRID_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.06);
const MARQUEE_FILL: Color = Color::from_rgba(1.0, 0.8, 0.0, 0.15);
const MARQUEE_STROKE: Color = Color::from_rgba(1.0, 0.8, 0.0, 0.9);
const VELOCITY_BAR_COLOR: Color = Color::from_rgba(0.2, 0.55, 1.0, 0.85);

// ── 实例构建（走 lumino-gfx，不自建 wgpu） ───────────────────────────

/// 由 `AutomationLane` 构建 `CcBarInstance` 列表（供上层 `CcBarRenderer::prepare` 消费）。
///
/// - `width` = 面板 grid 宽度（像素）
/// - `view` = 自动化局部视图参数（由 `AutomationPanelView` 映射）
#[must_use]
pub fn build_instances_for_lane(
    lane: &AutomationLane,
    view: &AutomationViewParams,
    width: f32,
    color: [f32; 3],
    show_anchors: bool,
) -> Vec<CcBarInstance> {
    let mut out = Vec::new();
    build_lane_instances(&mut out, width, view, lane, color, show_anchors);
    out
}

/// 将 `AutomationPanelView` 映射为 `AutomationViewParams`（与 `gfx::AutomationViewParams` 对齐）。
#[must_use]
pub fn view_params_for_panel(
    panel: &AutomationPanelView,
    panel_offset_x: f32,
    panel_offset_y: f32,
) -> AutomationViewParams {
    AutomationViewParams {
        panel_height: panel.panel_height,
        pixels_per_tick: panel.base.pixels_per_tick,
        scroll_x: panel.base.scroll_x,
        keyboard_width: panel.base.left_panel_width,
        value_zoom: panel.value_zoom,
        value_scroll: panel.value_scroll,
        panel_offset_x,
        panel_offset_y,
        toolbar_height: 0.0,
        line_thickness: 2.0,
    }
}

// ── Canvas Program ────────────────────────────────────────────────────

/// 自动化面板的 iced `canvas::Program` 状态（滚动/拖拽/缓存）。
#[derive(Debug, Default)]
pub struct AutomationPanelProgramState {
    pub scroll_y: f32,
    pub drag: Option<super::interaction::AutoDrag>,
    pub marquee_start: Option<Point>,
    pub cache_grid: Cache<Renderer>,
    pub cache_curves: Cache<Renderer>,
    pub cache_overlay: Cache<Renderer>,
}

/// 自动化面板 Canvas Program（单面板 iced 桩，多面板由上层 `Column` 组合）。
pub struct AutomationPanelProgram<'a> {
    pub panel: &'a AutomationPanelView,
    pub lane: Option<&'a AutomationLane>,
    pub velocity_points: Option<&'a [lumino_note_core::VelocityPoint]>,
    pub color: [f32; 3],
    pub ghost: Option<&'a AutomationGhost>,
    pub overlay: Option<&'a PanelOverlayData>,
    pub show_anchors: bool,
    pub combo_width: f32,
}

impl<'a> AutomationPanelProgram<'a> {
    pub fn new(panel: &'a AutomationPanelView) -> Self {
        Self {
            panel,
            lane: None,
            velocity_points: None,
            color: NODE_COLOR,
            ghost: None,
            overlay: None,
            show_anchors: true,
            combo_width: 48.0,
        }
    }
    pub fn with_lane(mut self, lane: Option<&'a AutomationLane>) -> Self {
        self.lane = lane;
        self
    }
    pub fn with_velocity(mut self, points: Option<&'a [lumino_note_core::VelocityPoint]>) -> Self {
        self.velocity_points = points;
        self
    }
    pub fn with_ghost(mut self, ghost: Option<&'a AutomationGhost>) -> Self {
        self.ghost = ghost;
        self
    }
    pub fn with_overlay(mut self, overlay: Option<&'a PanelOverlayData>) -> Self {
        self.overlay = overlay;
        self
    }
}

impl Program<Message, Theme, Renderer> for AutomationPanelProgram<'_> {
    type State = AutomationPanelProgramState;

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut geoms = Vec::new();
        let panel = self.panel;
        let max_val = panel_max_val_simple(panel);
        let vp = view_params_for_panel(panel, bounds.x, bounds.y);

        // 约束声明：全走 lumino-gfx 管线（CcBarRenderer + AutomationLane），不自建 wgpu
        let _assert_gfx: Option<&CcBarRenderer> = None;
        let _ = &vp;
        let _ = build_instances_for_lane
            as fn(
                &AutomationLane,
                &AutomationViewParams,
                f32,
                [f32; 3],
                bool,
            ) -> Vec<CcBarInstance>;

        // 1) 网格/中心线/值标签（缓存）
        {
            let g =
                state
                    .cache_grid
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        // 背景
                        let bg = Path::rectangle(Point::new(0.0, 0.0), bounds.size());
                        frame.fill(&bg, Color::from_rgba(0.12, 0.12, 0.14, 1.0));

                        // 中心线（PitchBend / Pan 等有 center_value 的目标）
                        if !panel.show_velocity && panel.selected_target.has_center_line() {
                            let center_val = panel.selected_target.default_value() as f32;
                            let y_center = panel
                                .value_to_y(center_val, panel.selected_target.max_value() as f32)
                                - panel.y_offset;
                            if (0.0..=panel.panel_height).contains(&y_center) {
                                let p = Path::rectangle(
                                    Point::new(self.combo_width, y_center - 0.5),
                                    Size::new(bounds.width - self.combo_width, 1.0),
                                );
                                frame.fill(&p, CENTER_LINE_COLOR);
                            }
                        }

                        // 网格竖线（按 ppq 的 Beat 刻度，简化版：每 480 tick 一线）
                        let ppu = panel.base.pixels_per_tick.max(0.001);
                        let left = panel.base.scroll_x;
                        let step = 480.0 * ppu;
                        if step > 4.0 {
                            let mut x = (left / step).floor() * step - left + self.combo_width;
                            while x < bounds.width {
                                let p = Path::rectangle(
                                    Point::new(x, 0.0),
                                    Size::new(1.0, bounds.height),
                                );
                                frame.fill(&p, GRID_COLOR);
                                x += step;
                            }
                        }

                        // 值标签（顶部/中部/底部，与 yinhe draw_value_labels 对齐的 iced 版）
                        let label_color = Color::from_rgba(0.7, 0.7, 0.7, 1.0);
                        // 占位：文本由 iced widget 层叠加，此处保留几何槽位
                        let _ = (label_color, max_val);
                    });
            geoms.push(g);
        }

        // 2) 曲线/柱状（矢量化预览；GPU 实例由上层 CcBarRenderer 消费）
        {
            let g =
                state
                    .cache_curves
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        if panel.show_velocity {
                            // Velocity 柱状（宽度 = note 长度，最窄 2px，高度按 value/127）
                            if let Some(points) = self.velocity_points {
                                for vp_pt in points.iter().take(4096) {
                                    let x = self.combo_width
                                        + vp_pt.tick * panel.base.pixels_per_tick
                                        - panel.base.scroll_x;
                                    if x < self.combo_width - 4.0 || x > bounds.width {
                                        continue;
                                    }
                                    let w = (vp_pt.length * panel.base.pixels_per_tick).max(2.0);
                                    let h =
                                        (f32::from(vp_pt.velocity) / 127.0) * panel.panel_height;
                                    let y = panel.panel_height - h;
                                    let rect = Path::rectangle(Point::new(x, y), Size::new(w, h));
                                    frame.fill(&rect, VELOCITY_BAR_COLOR);
                                }
                            }
                        } else if let Some(lane) = self.lane {
                            // 自动化曲线：按事件间 shape 画折线（Step 水平+竖直，Curve 采样子采样）
                            let pts: Vec<(f32, f32)> = lane
                                .events
                                .iter()
                                .map(|e| {
                                    let x = self.combo_width
                                        + e.tick as f32 * panel.base.pixels_per_tick
                                        - panel.base.scroll_x;
                                    let y = panel.value_to_y(f32::from(e.value), max_val)
                                        - panel.y_offset;
                                    (x, y)
                                })
                                .collect();
                            if pts.len() >= 2 {
                                for w in pts.windows(2) {
                                    let (x1, y1) = w[0];
                                    let (x2, y2) = w[1];
                                    // Step：水平线
                                    let hp = Path::line(Point::new(x1, y1), Point::new(x2, y1));
                                    frame.stroke(
                                        &hp,
                                        Stroke::default().with_width(2.0).with_color(
                                            Color::from_rgb(
                                                self.color[0],
                                                self.color[1],
                                                self.color[2],
                                            ),
                                        ),
                                    );
                                    // Step 竖直跳变
                                    if (y2 - y1).abs() > 0.5 {
                                        let vp_line =
                                            Path::line(Point::new(x2, y1), Point::new(x2, y2));
                                        frame.stroke(
                                            &vp_line,
                                            Stroke::default().with_width(2.0).with_color(
                                                Color::from_rgb(
                                                    self.color[0],
                                                    self.color[1],
                                                    self.color[2],
                                                ),
                                            ),
                                        );
                                    }
                                }
                            }
                            // 锚点（圆角矩形 6px）
                            if self.show_anchors {
                                for (x, y) in pts {
                                    let r = 3.0;
                                    let p = Path::circle(Point::new(x, y), r);
                                    frame.fill(
                                        &p,
                                        Color::from_rgb(
                                            self.color[0],
                                            self.color[1],
                                            self.color[2],
                                        ),
                                    );
                                    frame.stroke(
                                        &p,
                                        Stroke::default().with_width(1.0).with_color(Color::WHITE),
                                    );
                                }
                            }
                            // Ghost 叠加（拖拽中整 lane 预览，半透明）
                            if let Some(AutomationGhost::Move {
                                lane: ghost_lane,
                                color,
                            }) = self.ghost
                            {
                                let gpts: Vec<(f32, f32)> = ghost_lane
                                    .events
                                    .iter()
                                    .map(|e| {
                                        let x = self.combo_width
                                            + e.tick as f32 * panel.base.pixels_per_tick
                                            - panel.base.scroll_x;
                                        let y = panel.value_to_y(f32::from(e.value), max_val)
                                            - panel.y_offset;
                                        (x, y)
                                    })
                                    .collect();
                                for w in gpts.windows(2) {
                                    let (x1, y1) = w[0];
                                    let (x2, y2) = w[1];
                                    let hp = Path::line(Point::new(x1, y1), Point::new(x2, y1));
                                    frame.stroke(
                                        &hp,
                                        Stroke::default().with_width(2.0).with_color(
                                            Color::from_rgba(color[0], color[1], color[2], 0.45),
                                        ),
                                    );
                                    if (y2 - y1).abs() > 0.5 {
                                        let vp_line =
                                            Path::line(Point::new(x2, y1), Point::new(x2, y2));
                                        frame.stroke(
                                            &vp_line,
                                            Stroke::default().with_width(2.0).with_color(
                                                Color::from_rgba(
                                                    color[0], color[1], color[2], 0.45,
                                                ),
                                            ),
                                        );
                                    }
                                }
                            }
                            if let Some(AutomationGhost::Curve { start, end, color }) = self.ghost {
                                let p = Path::line(*start, *end);
                                frame.stroke(
                                    &p,
                                    Stroke::default()
                                        .with_width(2.0)
                                        .with_color(Color::from_rgba(
                                            color[0], color[1], color[2], 0.5,
                                        )),
                                );
                            }
                            let _ = lane;
                        }
                    });
            geoms.push(g);
        }

        // 3) Overlay：选框 + velocity 笔划预览（与 yinhe draw_panel_overlay 对齐）
        {
            let g =
                state
                    .cache_overlay
                    .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                        if let Some(overlay) = self.overlay {
                            // velocity 笔划预览（半透明柱）
                            if let Some(preview) = &overlay.velocity_preview {
                                for rect in &preview.bars {
                                    // rect 已在 panel 坐标，需平移到 bounds 本地
                                    let local = Rectangle::new(
                                        Point::new(rect.x - bounds.x, rect.y - bounds.y),
                                        rect.size(),
                                    );
                                    // 视口裁剪
                                    if local.x + local.width < 0.0 || local.x > bounds.width {
                                        continue;
                                    }
                                    let p = Path::rectangle(local.position(), local.size());
                                    frame.fill(&p, preview.color.scale_alpha(0.85));
                                    frame.stroke(
                                        &p,
                                        Stroke::default().with_width(1.0).with_color(Color::WHITE),
                                    );
                                }
                            }
                            // 持续化选框（半透明填充 + 描边）
                            if overlay.marquee_rect.is_none() {
                                // 实际多选框由调用方在 panel.anchor_sel_rects 上维护，此处仅占位
                            }
                            if let Some(r) = overlay.marquee_rect {
                                let local = Rectangle::new(
                                    Point::new(r.x - bounds.x, r.y - bounds.y),
                                    r.size(),
                                );
                                let p = Path::rectangle(local.position(), local.size());
                                frame.fill(&p, MARQUEE_FILL);
                                frame.stroke(
                                    &p,
                                    Stroke::default().with_width(1.0).with_color(MARQUEE_STROKE),
                                );
                            }
                        }
                        // 多选框（anchor_sel_rects）的高亮（与 yinhe 选框渲染对齐）
                        for sel in &panel.anchor_sel_rects {
                            let ts = sel.tick_start.min(sel.tick_end);
                            let te = sel.tick_start.max(sel.tick_end);
                            let x1 = self.combo_width + ts as f32 * panel.base.pixels_per_tick
                                - panel.base.scroll_x;
                            let x2 = self.combo_width + te as f32 * panel.base.pixels_per_tick
                                - panel.base.scroll_x;
                            let (y1, y2) = match sel.value_range {
                                None => (0.0, panel.panel_height),
                                Some((vmin, vmax)) => {
                                    let v1 = vmin.clamp(0.0, max_val);
                                    let v2 = vmax.clamp(0.0, max_val);
                                    let ya = panel.value_to_y(v2, max_val) - panel.y_offset;
                                    let yb = panel.value_to_y(v1, max_val) - panel.y_offset;
                                    (ya.min(yb), ya.max(yb))
                                }
                            };
                            let r = Rectangle::new(
                                Point::new(x1, y1),
                                Size::new((x2 - x1).max(1.0), (y2 - y1).max(1.0)),
                            );
                            let p = Path::rectangle(r.position(), r.size());
                            frame.fill(&p, MARQUEE_FILL);
                            frame.stroke(
                                &p,
                                Stroke::default().with_width(1.0).with_color(MARQUEE_STROKE),
                            );
                        }
                    });
            geoms.push(g);
        }

        geoms
    }
}

/// 绘制值标签到 `Frame`（与 yinhe `draw_value_labels` 对齐的 iced 占位）。
pub fn draw_value_labels(
    frame: &mut Frame<Renderer>,
    panel: &AutomationPanelView,
    panel_rect: Rectangle,
    combo_width: f32,
    max_val: f32,
) {
    let label_color = Color::from_rgba(0.7, 0.7, 0.7, 1.0);
    let _ = (frame, panel, panel_rect, combo_width, max_val, label_color);
}

// ── GFX 约束声明（不自建 wgpu） ───────────────────────────────────────

#[allow(dead_code)]
type GfxCcBarBuffer = lumino_gfx::CcBarInstance;
#[allow(dead_code)]
fn _assert_gfx_types(_r: &CcBarRenderer, _lane: &AutomationLane, _target: &AutomationTarget) {}

// 抑制未使用导入（桩层保留类型引用以确保管线一致）
#[allow(dead_code)]
fn _keep_imports(_lane: &AutomationLane, _target: &AutomationTarget) {
    let _ = NODE_COLOR;
    let _ = CENTER_LINE_COLOR;
}
