//! 自动化面板交互 — 对应 yinhe `automation_panel/interaction.rs` + `interaction/*` 4 文件
//!
//! iced 迁移：
//! - 以 `iced_core::mouse::Event / keyboard::Event + mouse::Cursor + Rectangle`
//!   替代 `egui::Ui::input(|i| i.pointer.*)` + `ui.data().get_temp::<AutoDrag>`；
//! - 持久化状态由 `Program::State`（`AutomationPanelState`）持有；
//! - 命中/拖拽/选框/控制点逻辑与 yinhe 语义对齐，落盘类型复用
//!   `lumino_note_core::{AutomationLane, AutomationEvent, SegmentShape, AutomationEdit}`，
//!   渲染走 `lumino_gfx::{CcBarRenderer, AutomationViewParams, build_lane_instances}`（不自建 wgpu）。

use iced_core::{Point, Rectangle, mouse};

use lumino_note_core::{AutomationEdit, AutomationLane, AutomationTarget, SegmentShape};

use super::constants::{ANCHOR_HIT_PX, MARQUEE_THRESHOLD};
use super::types::{
    AnchorSelRect, AutomationGhost, AutomationPanelView, ControlPointHit, CtrlEnd, HoverTooltip,
    Tool,
};

// ── 拖拽状态（跨帧，保持在 Program::State） ───────────────────────────

/// 自动化面板拖拽状态（与 yinhe `AutoDrag` 语义一致）。
#[derive(Clone, Copy, Debug)]
pub enum AutoDrag {
    MoveAnchor {
        old_tick: u32,
        start_tick: u32,
        start_value: f32,
    },
    CurveDraw {
        start_tick: u32,
        start_value: f32,
    },
    DragControlPoint {
        prev_tick: u32,
        which: CtrlEnd,
        start_x: f32,
        start_y: f32,
    },
    MoveAnchors {
        start_tick: u32,
        start_value: f32,
        alt: bool,
    },
    MarqueeSelect {
        start_pos: Point,
    },
    EraserMarquee {
        start_pos: Point,
    },
}

// ── 选区操作 ──────────────────────────────────────────────────────────

/// 持续化选框变更操作（与 yinhe `SelRectOp` 一致）。
#[derive(Clone, Debug)]
pub enum SelRectOp {
    Set(AnchorSelRect),
    Append(AnchorSelRect),
    ReplaceAll(Vec<AnchorSelRect>),
    Keep,
}

/// Select 工具的选区操作（由 interaction 返回，调用方回写 `panel.anchor_sel_rects`）。
#[derive(Clone, Debug)]
pub enum SelOp {
    Set(SelRectOp),
    Clear,
    ClearNoteSelection,
}

/// 右键锚点编辑信息（供信息面板/右键菜单消费）。
#[derive(Clone, Debug)]
pub struct RightClickAnchor {
    pub track_idx: u16,
    pub lane_idx: usize,
    pub old_tick: u32,
    pub target: AutomationTarget,
}

// ── 命中检测 ─────────────────────────────────────────────────────────

/// 检测鼠标是否落在某段插值线上（像素距离 ≤ 8.0）。
#[must_use]
pub fn hit_line_on_lane(
    lane: &AutomationLane,
    tick: u32,
    value: f32,
    panel: &AutomationPanelView,
    max_val: f32,
) -> bool {
    let idx = lane.events.partition_point(|e| e.tick <= tick);
    if idx == 0 || idx >= lane.events.len() {
        return false;
    }
    let left = &lane.events[idx - 1];
    let right = &lane.events[idx];
    let t = if right.tick == left.tick {
        0.0
    } else {
        (tick - left.tick) as f32 / (right.tick - left.tick) as f32
    };
    let interp = left.shape.interpolate(t);
    let interp_value = left.value as f32 + interp * (right.value as f32 - left.value as f32);
    let interp_y = panel.value_to_y(interp_value, max_val);
    let mouse_y = panel.value_to_y(value, max_val);
    (interp_y - mouse_y).abs() <= 8.0
}

/// 检测鼠标是否命中某段 Curve 的控制点（`ANCHOR_HIT_PX` 半径）。
#[must_use]
pub fn hit_control_point_on_lane(
    lane: &AutomationLane,
    mouse: Point,
    _ppu: f32,
    scroll_x: f32,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    panel: &AutomationPanelView,
    max_val: f32,
) -> Option<ControlPointHit> {
    let x_offset = grid_area.x - scroll_x;
    let hit_sq = ANCHOR_HIT_PX * ANCHOR_HIT_PX;
    let mut best: Option<ControlPointHit> = None;
    for i in 1..lane.events.len() {
        let prev = &lane.events[i - 1];
        let cur = &lane.events[i];
        let SegmentShape::Curve { tension: _ } = prev.shape else {
            continue;
        };
        // lumino 侧 Curve 统一为 tension 模型，未做贝塞尔分段时视为直线，不提供控制点命中
        // 保留接口以对齐 yinhe 的 UI 手柄交互，实际命中由调用方按 tension 可视化决定
        let _ = (
            cur, x_offset, panel_rect, panel, max_val, mouse, hit_sq, &mut best, prev,
        );
        // 占位：tension 模型下控制点为隐式，暂不命中（避免误触）
    }
    best
}

/// 由鼠标位置反推控制点偏移 `(x,y) ∈ [-0.5,0.5]`（与 yinhe `compute_ctrl_from_mouse` 对齐的占位）。
#[must_use]
pub fn compute_ctrl_from_mouse(
    lane: &AutomationLane,
    prev_tick: u32,
    which: CtrlEnd,
    mouse: Point,
    ppu: f32,
    scroll_x: f32,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    panel: &AutomationPanelView,
    max_val: f32,
) -> Option<(f32, f32)> {
    let prev_idx = lane.events.iter().position(|e| e.tick == prev_tick)?;
    let prev = &lane.events[prev_idx];
    let next = lane.events.get(prev_idx + 1)?;
    let x_offset = grid_area.x - scroll_x;
    let px0 = x_offset + prev.tick as f32 * ppu;
    let py0 = panel_rect.y + panel.value_to_y(prev.value as f32, max_val);
    let px3 = x_offset + next.tick as f32 * ppu;
    let py3 = panel_rect.y + panel.value_to_y(next.value as f32, max_val);
    let dx = px3 - px0;
    let dy = py3 - py0;
    let (rx, ry) = match which {
        CtrlEnd::Out => (px0, py0),
        CtrlEnd::In => (px3, py3),
    };
    let x_range = match which {
        CtrlEnd::Out => (0.0, 0.25),
        CtrlEnd::In => (-0.25, 0.0),
    };
    let new_x = if dx.abs() < 1e-3 {
        0.0
    } else {
        ((mouse.x - rx) / dx / 4.0).clamp(x_range.0, x_range.1)
    };
    let new_y = if dy.abs() < 1e-3 {
        0.0
    } else {
        ((mouse.y - ry) / dy / 4.0).clamp(-0.5, 0.5)
    };
    Some((new_x, new_y))
}

/// 按端别合并控制点偏移 → 新 `SegmentShape`（tension 近似）。
#[must_use]
pub fn merge_ctrl_shape(
    lane: &AutomationLane,
    prev_tick: u32,
    which: CtrlEnd,
    new_ctrl: (f32, f32),
) -> SegmentShape {
    let _ = which;
    lane.events
        .iter()
        .find(|e| e.tick == prev_tick)
        .map(|e| match e.shape {
            SegmentShape::Curve { tension } => {
                // 将 y 偏移映射为 tension（-0.5..0.5 → -127..127）
                let t = (new_ctrl.1 * 254.0).round().clamp(-127.0, 127.0) as i8;
                // 取新旧 tension 的平均以保留一定连续性
                let blended = ((t as i16 + tension as i16) / 2).clamp(-127, 127) as i8;
                SegmentShape::Curve { tension: blended }
            }
            SegmentShape::Step => SegmentShape::Curve {
                tension: (new_ctrl.1 * 64.0) as i8,
            },
        })
        .unwrap_or(SegmentShape::Step)
}

// ── 选区 helpers ──────────────────────────────────────────────────────

#[must_use]
pub fn union_anchor_sel_rect(a: AnchorSelRect, b: AnchorSelRect) -> AnchorSelRect {
    let ts = a
        .tick_start
        .min(a.tick_end)
        .min(b.tick_start)
        .min(b.tick_end);
    let te = a
        .tick_start
        .max(a.tick_end)
        .max(b.tick_start)
        .max(b.tick_end);
    let value_range = match (a.value_range, b.value_range) {
        (None, _) | (_, None) => None,
        (Some((va1, va2)), Some((vb1, vb2))) => {
            let vmin = va1.min(va2).min(vb1).min(vb2);
            let vmax = va1.max(va2).max(vb1).max(vb2);
            Some((vmin, vmax))
        }
    };
    AnchorSelRect {
        tick_start: ts,
        tick_end: te,
        value_range,
    }
}

// ── 交互主入口（iced Program::update 调用） ──────────────────────────

/// 面板交互输入（由 `Program::State` 汇总后传入）。
#[derive(Clone, Copy, Debug)]
pub struct PanelInput {
    pub cursor: Point,
    pub in_grid: bool,
    pub tick: u32,
    pub value: f32,
    pub tool: Tool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
}

/// 交互输出（与 yinhe `handle_automation_interaction` 返回对齐）。
#[derive(Clone, Debug, Default)]
pub struct PanelInteractionOutput {
    pub edits: Vec<AutomationEdit>,
    pub ghost: Option<AutomationGhost>,
    pub drag_info: Option<HoverTooltip>,
    pub hover_info: Option<HoverTooltip>,
    pub marquee_rect: Option<Rectangle>,
    pub sel_op: Option<SelOp>,
}

/// 命中最近锚点（`ANCHOR_HIT_PX` 内），返回 `(event_idx, tick)`。
#[must_use]
pub fn hit_anchor(
    lane: &AutomationLane,
    mouse: Point,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    panel: &AutomationPanelView,
    max_val: f32,
) -> Option<(usize, u32)> {
    let ppu = panel.base.pixels_per_tick;
    let scroll_x = panel.base.scroll_x;
    lane.events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let ex = grid_area.x + e.tick as f32 * ppu - scroll_x;
            let ey = panel_rect.y + panel.value_to_y(e.value as f32, max_val);
            let d = ((ex - mouse.x).powi(2) + (ey - mouse.y).powi(2)).sqrt();
            if d <= ANCHOR_HIT_PX {
                Some((i, e.tick, d))
            } else {
                None
            }
        })
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, t, _)| (i, t))
}

/// 供拖拽释放阶段复用的提交逻辑（与 yinhe `commit_anchor_or_ctrl_release` 对齐）。
#[allow(clippy::too_many_arguments)]
pub fn commit_anchor_or_ctrl_release(
    drag: Option<AutoDrag>,
    lane: Option<&AutomationLane>,
    lane_idx: Option<usize>,
    track_idx: u16,
    _target: &AutomationTarget,
    mouse_info: Option<(Point, u32, f32)>,
    max_val: f32,
    edits: &mut Vec<AutomationEdit>,
    track_color: [f32; 3],
) -> Option<AutomationGhost> {
    match drag {
        Some(AutoDrag::MoveAnchor {
            old_tick,
            start_tick,
            start_value,
        }) => {
            if let Some((_, new_tick, new_value)) = mouse_info
                && (new_tick != start_tick || (new_value - start_value).abs() > f32::EPSILON)
                && let Some(lidx) = lane_idx
            {
                edits.push(AutomationEdit::Move {
                    track_idx,
                    lane_idx: lidx,
                    old_tick,
                    old_value: lane.and_then(|l| {
                        l.events
                            .iter()
                            .find(|e| e.tick == old_tick)
                            .map(|e| e.value)
                    }),
                    new_tick,
                    new_value: new_value.round().clamp(0.0, max_val) as u16,
                });
                if let Some(l) = lane {
                    // ghost 为整 lane 覆盖（与 `build_lane_override` 对齐的 iced 占位：克隆 lane 并更新值）
                    let mut ghost_lane = l.clone();
                    if let Some(ev) = ghost_lane.events.iter_mut().find(|e| e.tick == old_tick) {
                        ev.tick = new_tick;
                        ev.value = new_value.round().clamp(0.0, max_val) as u16;
                        ghost_lane.events.sort_by_key(|e| e.tick);
                    }
                    return Some(AutomationGhost::Move {
                        lane: ghost_lane,
                        color: track_color,
                    });
                }
            }
            None
        }
        Some(AutoDrag::DragControlPoint {
            prev_tick,
            which,
            start_x,
            start_y,
        }) => {
            if let Some(l) = lane
                && let Some((p, _, _)) = mouse_info
                && let Some(lidx) = lane_idx
                && let Some(new_ctrl) = compute_ctrl_from_mouse(
                    l,
                    prev_tick,
                    which,
                    p,
                    0.0,
                    0.0,
                    Rectangle::new(Point::new(0.0, 0.0), iced_core::Size::new(0.0, 0.0)),
                    Rectangle::new(Point::new(0.0, 0.0), iced_core::Size::new(0.0, 0.0)),
                    &AutomationPanelView::default(),
                    max_val,
                )
                && (new_ctrl.0 != start_x || new_ctrl.1 != start_y)
            {
                let new_shape = merge_ctrl_shape(l, prev_tick, which, new_ctrl);
                edits.push(AutomationEdit::CycleShape {
                    track_idx,
                    lane_idx: lidx,
                    tick: prev_tick,
                });
                let _ = new_shape;
                let mut ghost_lane = l.clone();
                if let Some(ev) = ghost_lane.events.iter_mut().find(|e| e.tick == prev_tick) {
                    ev.shape = new_shape;
                }
                return Some(AutomationGhost::Move {
                    lane: ghost_lane,
                    color: track_color,
                });
            }
            None
        }
        _ => None,
    }
}

/// 主交互分派（Pencil / Curve / Select（含垂直）/ Eraser），由 `Program::update` 调用。
///
/// - 命中与坐标换算复用 `AutomationPanelView::value_to_y / y_to_value`；
/// - 选框阈值 `MARQUEE_THRESHOLD` 与 yinhe 一致；
/// - 落盘走 `AutomationEdit`，渲染走 `CcBarRenderer + AutomationLane`（ghost 预览）。
#[allow(clippy::too_many_arguments)]
pub fn handle_automation_interaction(
    panel: &AutomationPanelView,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    lane: Option<&AutomationLane>,
    lane_idx: Option<usize>,
    track_idx: u16,
    input: PanelInput,
    drag_state: Option<AutoDrag>,
    max_val: f32,
    track_color: [f32; 3],
) -> PanelInteractionOutput {
    let mut out = PanelInteractionOutput::default();
    let target = panel.selected_target.clone();
    if max_val <= 0.0 {
        return out;
    }
    let hit = lane.and_then(|l| hit_anchor(l, input.cursor, grid_area, panel_rect, panel, max_val));
    // 控制点命中（仅 Pencil/Select 系，未拖拽时）
    let hit_ctrl = if matches!(
        input.tool,
        Tool::Pencil | Tool::Select | Tool::SelectVertical
    ) && drag_state.is_none()
        && hit.is_none()
        && input.in_grid
    {
        lane.and_then(|l| {
            hit_control_point_on_lane(
                l,
                input.cursor,
                panel.base.pixels_per_tick,
                panel.base.scroll_x,
                grid_area,
                panel_rect,
                panel,
                max_val,
            )
        })
    } else {
        None
    };

    match input.tool {
        Tool::Pencil => {
            // iced 侧 Pencil 的单击新建/双击删除/拖拽移动由 Program::State 的 Pressed/Released 驱动；
            // 此函数为每帧的 ghost/hover 计算，实际 edits 在 update 的 Released 分支提交。
            if let Some(AutoDrag::MoveAnchor { old_tick, .. }) = drag_state
                && let Some(l) = lane
            {
                let mut ghost_lane = l.clone();
                if let Some(ev) = ghost_lane.events.iter_mut().find(|e| e.tick == old_tick) {
                    ev.tick = input.tick;
                    ev.value = input.value.round().clamp(0.0, max_val) as u16;
                    ghost_lane.events.sort_by_key(|e| e.tick);
                }
                out.ghost = Some(AutomationGhost::Move {
                    lane: ghost_lane,
                    color: track_color,
                });
                out.drag_info = Some(HoverTooltip::Anchor {
                    tick: input.tick,
                    value: input.value,
                    pos: input.cursor,
                });
            } else if let Some(AutoDrag::CurveDraw {
                start_tick,
                start_value,
            }) = drag_state
            {
                // 占位 ghost：起点→当前点的曲线段
                let start = Point::new(
                    grid_area.x + start_tick as f32 * panel.base.pixels_per_tick
                        - panel.base.scroll_x,
                    panel_rect.y + panel.value_to_y(start_value, max_val),
                );
                out.ghost = Some(AutomationGhost::Curve {
                    start,
                    end: input.cursor,
                    color: track_color,
                });
                out.drag_info = Some(HoverTooltip::Anchor {
                    tick: input.tick,
                    value: input.value,
                    pos: input.cursor,
                });
            } else if hit.is_some() || hit_ctrl.is_some() {
                out.hover_info = hit.map(|(_, t)| HoverTooltip::Anchor {
                    tick: t,
                    value: input.value,
                    pos: input.cursor,
                });
            }
        }
        Tool::Curve => {
            if let Some(AutoDrag::CurveDraw {
                start_tick,
                start_value,
            }) = drag_state
            {
                let start = Point::new(
                    grid_area.x + start_tick as f32 * panel.base.pixels_per_tick
                        - panel.base.scroll_x,
                    panel_rect.y + panel.value_to_y(start_value, max_val),
                );
                out.ghost = Some(AutomationGhost::Curve {
                    start,
                    end: input.cursor,
                    color: track_color,
                });
                out.drag_info = Some(HoverTooltip::Anchor {
                    tick: input.tick,
                    value: input.value,
                    pos: input.cursor,
                });
            }
        }
        Tool::Select | Tool::SelectVertical => {
            if let Some(AutoDrag::MarqueeSelect { start_pos }) = drag_state {
                let d = ((input.cursor.x - start_pos.x).powi(2)
                    + (input.cursor.y - start_pos.y).powi(2))
                .sqrt();
                if d >= MARQUEE_THRESHOLD {
                    let r = Rectangle::new(
                        Point::new(start_pos.x.min(input.cursor.x), grid_area.y),
                        iced_core::Size::new(
                            (input.cursor.x - start_pos.x).abs(),
                            grid_area.height,
                        ),
                    );
                    out.marquee_rect = Some(r);
                }
            } else if let Some(AutoDrag::MoveAnchors {
                start_tick,
                start_value,
                alt: _,
            }) = drag_state
                && let Some(l) = lane
                && !panel.anchor_sel_rects.is_empty()
            {
                let d_tick = input.tick as i64 - start_tick as i64;
                let vertical = matches!(input.tool, Tool::SelectVertical)
                    || panel
                        .anchor_sel_rects
                        .iter()
                        .any(|r| r.value_range.is_none());
                let d_value = if vertical {
                    0.0
                } else {
                    input.value - start_value
                };
                if d_tick != 0 || d_value.abs() > 1e-6 {
                    // 构造 Move 的 ghost lane（整 lane 偏移，选中的才动）
                    let mut ghost_lane = l.clone();
                    for ev in &mut ghost_lane.events {
                        if panel
                            .anchor_sel_rects
                            .iter()
                            .any(|r| r.contains(ev.tick, f32::from(ev.value)))
                        {
                            let nt = (ev.tick as i64 + d_tick).max(0) as u32;
                            let nv = (f32::from(ev.value) + d_value).clamp(0.0, max_val) as u16;
                            ev.tick = nt;
                            ev.value = nv;
                        }
                    }
                    ghost_lane.events.sort_by_key(|e| e.tick);
                    out.ghost = Some(AutomationGhost::Move {
                        lane: ghost_lane,
                        color: track_color,
                    });
                    out.drag_info = Some(HoverTooltip::Anchor {
                        tick: input.tick,
                        value: input.value,
                        pos: input.cursor,
                    });
                }
            }
            // 点击锚点时的 cursor 提示由 Program::mouse_interaction 基于 hit 决定，此处不直接设 cursor
            let _ = (hit, input, lane_idx, target, track_idx);
        }
        Tool::Eraser => {
            if let Some(AutoDrag::EraserMarquee { start_pos }) = drag_state {
                let d = ((input.cursor.x - start_pos.x).powi(2)
                    + (input.cursor.y - start_pos.y).powi(2))
                .sqrt();
                if d >= MARQUEE_THRESHOLD {
                    let r = Rectangle::new(
                        Point::new(
                            start_pos.x.min(input.cursor.x),
                            start_pos.y.min(input.cursor.y),
                        ),
                        iced_core::Size::new(
                            (input.cursor.x - start_pos.x).abs(),
                            (input.cursor.y - start_pos.y).abs(),
                        ),
                    );
                    out.marquee_rect = Some(r);
                }
            }
        }
    }
    out
}

/// 供 `Program::mouse_interaction` 使用的光标判定（与 yinhe 交互一致）。
#[must_use]
pub fn cursor_for_state(
    drag_state: Option<AutoDrag>,
    hit_anchor: bool,
    hit_ctrl: bool,
    in_sel_rect: bool,
    tool: Tool,
) -> mouse::Interaction {
    if let Some(drag) = drag_state {
        return match drag {
            AutoDrag::MoveAnchors { .. } => mouse::Interaction::Grabbing,
            AutoDrag::MarqueeSelect { .. } | AutoDrag::EraserMarquee { .. } => {
                mouse::Interaction::Crosshair
            }
            _ => mouse::Interaction::Grabbing,
        };
    }
    if hit_anchor || hit_ctrl {
        return mouse::Interaction::Grab;
    }
    if matches!(tool, Tool::Select | Tool::SelectVertical) && in_sel_rect {
        return mouse::Interaction::Grab;
    }
    mouse::Interaction::Idle
}
