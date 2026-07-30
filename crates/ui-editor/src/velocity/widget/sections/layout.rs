//! 布局相关方法：坐标转换、命中测试、resize 区域检测

use iced_core::{Point, Size};
use lumino_core::{AutomationLane, AutomationTarget};
use lumino_gfx::automation::AutomationViewParams;

use super::super::super::{
    HIT_RADIUS, PANEL_PADDING_Y, RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT, VelocityPoint,
};
use super::super::state::CtrlEnd;
use crate::editor_state::ViewState;
use crate::velocity::EditMode;
use crate::velocity::widget::TempoPoint;

impl super::super::VelocityCanvas<'_> {
    /// 获取所有力度点
    ///
    /// **性能优化**：NoteStore 启用时走 `build_velocity_points_from_store`，
    /// 10M+ 音符场景下避免全部 Note clone 开销（百毫秒级）。
    pub(super) fn points(&self) -> Vec<VelocityPoint> {
        let data = &self.editor.editor_state.data;
        if data.is_note_store_enabled() {
            super::super::super::VelocityPanel::build_velocity_points_from_store(&data.note_store)
        } else {
            super::super::super::VelocityPanel::build_velocity_points(&data.notes)
        }
    }

    /// 将力度值映射到 Y 坐标
    pub fn velocity_to_y(velocity: u8, bounds_height: f32) -> f32 {
        let max_y = bounds_height;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let normalized = velocity as f32 / 127.0;
        max_y - normalized * (max_y - min_y)
    }

    /// 将 Y 坐标映射回力度值 (0-127)
    pub fn y_to_velocity(y: f32, bounds_height: f32) -> u8 {
        let max_y = bounds_height;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let clamped_y = y.clamp(min_y, max_y);
        let normalized = (max_y - clamped_y) / (max_y - min_y);
        (normalized * 127.0).round().clamp(0.0, 127.0) as u8
    }

    /// 获取点的屏幕位置
    pub(crate) fn point_screen_pos(
        point: &VelocityPoint,
        _index: usize,
        _bounds_width: f32,
        bounds_height: f32,
        view: &ViewState,
    ) -> Point {
        let x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        let y = Self::velocity_to_y(point.velocity, bounds_height);
        Point::new(x, y)
    }

    /// 命中测试：寻找点击位置最近的力度点
    pub(super) fn hit_test(
        points: &[VelocityPoint],
        click_pos: Point,
        bounds_width: f32,
        bounds_height: f32,
        view: &ViewState,
    ) -> Option<usize> {
        let mut closest: Option<(usize, f32)> = None;
        for (i, point) in points.iter().enumerate() {
            let pos = Self::point_screen_pos(point, i, bounds_width, bounds_height, view);
            let dx = click_pos.x - pos.x;
            let dy = click_pos.y - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < HIT_RADIUS {
                match closest {
                    None => closest = Some((i, dist)),
                    Some((_, best_dist)) if dist < best_dist => closest = Some((i, dist)),
                    _ => {}
                }
            }
        }
        closest.map(|(idx, _)| idx)
    }

    /// 判断光标是否在 resize 手柄区域
    pub(super) fn is_in_resize_zone(cursor_pos: Point) -> bool {
        (0.0..=RESIZE_HANDLE_HEIGHT).contains(&cursor_pos.y)
    }

    /// 当前编辑模式对应的自动化目标。
    pub(super) fn automation_target(&self) -> Option<AutomationTarget> {
        match self.edit_mode {
            EditMode::Bend => Some(AutomationTarget::PitchBend),
            EditMode::Cc(n) => Some(AutomationTarget::CC { controller: n }),
            _ => None,
        }
    }

    /// 构造 Canvas 局部坐标系的自动化视图参数。
    pub(super) fn automation_view_params(
        &self,
        bounds_size: Size,
    ) -> Option<(AutomationViewParams, AutomationTarget, f32)> {
        let target = self.automation_target()?;
        let view = &self.editor.editor_state.view;
        let panel = &self.editor.velocity_panel;
        let params = AutomationViewParams {
            panel_height: bounds_size.height + TOOLBAR_HEIGHT,
            pixels_per_tick: view.zoom_x,
            scroll_x: view.scroll_x,
            keyboard_width: view.keyboard_width,
            value_zoom: panel.value_zoom,
            value_scroll: panel.value_scroll,
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            toolbar_height: TOOLBAR_HEIGHT,
            line_thickness: panel.automation_line_thickness,
        };
        let max_val = target.max_value() as f32;
        Some((params, target, max_val))
    }

    /// 获取当前音轨当前目标的自动化 lane（若存在）。
    pub(super) fn current_automation_lane(&self) -> Option<&AutomationLane> {
        let target = self.automation_target()?;
        let data = &self.editor.editor_state.data;
        let track = data.current_track as u16;
        data.find_automation_lane(track, &target)
            .and_then(|idx| data.automation_lanes.get(idx).map(|a| &**a))
    }

    /// 将 X 坐标转换为 tick（值空间）。
    pub(super) fn x_to_tick(&self, x: f32) -> f32 {
        let view = &self.editor.editor_state.view;
        (x - view.keyboard_width + view.scroll_x) / view.zoom_x
    }

    /// 吸附 tick。
    pub(super) fn snap_tick(&self, tick: f32) -> f32 {
        self.editor.snap_tick(tick)
    }

    /// 将 Y 坐标映射回 BPM 值 (20.0 ~ 10000.0)
    pub fn y_to_bpm(y: f32, bounds_height: f32) -> f64 {
        let max_y = bounds_height;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let clamped_y = y.clamp(min_y, max_y);
        let normalized = (max_y - clamped_y) / (max_y - min_y);
        let bpm_range = 10000.0 - 20.0;
        20.0 + normalized as f64 * bpm_range
    }

    /// 命中测试 Tempo 控制点
    pub(super) fn hit_test_tempo_point(
        points: &[TempoPoint],
        click_pos: Point,
        bounds_width: f32,
        bounds_height: f32,
        view: &ViewState,
    ) -> Option<usize> {
        use crate::velocity::widget::drawing::tempo_point_screen_pos;
        let mut closest: Option<(usize, f32)> = None;
        for (i, point) in points.iter().enumerate() {
            let pos =
                tempo_point_screen_pos(point, bounds_width, bounds_height, view, 20.0, 9980.0);
            let dx = click_pos.x - pos.x;
            let dy = click_pos.y - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < HIT_RADIUS {
                match closest {
                    None => closest = Some((i, dist)),
                    Some((_, best_dist)) if dist < best_dist => closest = Some((i, dist)),
                    _ => {}
                }
            }
        }
        closest.map(|(idx, _)| idx)
    }

    /// 命中测试：寻找点击位置最近的自动化锚点，返回其 tick。
    pub(super) fn hit_test_automation_anchor(
        lane: &AutomationLane,
        view: &AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> Option<u32> {
        let radius = HIT_RADIUS;
        let mut best: Option<(u32, f32)> = None;
        for evt in &lane.events {
            let x = view.tick_to_x(evt.tick);
            let y = view.value_to_y(evt.value as f32, max_val);
            let dx = cursor_pos.x - x;
            let dy = cursor_pos.y - y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < radius {
                match best {
                    None => best = Some((evt.tick, dist)),
                    Some((_, best_dist)) if dist < best_dist => best = Some((evt.tick, dist)),
                    _ => {}
                }
            }
        }
        best.map(|(tick, _)| tick)
    }

    /// 命中测试：检测鼠标是否在某个贝塞尔控制点上。
    /// 返回 (前驱事件 tick, 控制点端别, 该段 shape 的 4 个偏移量)。
    pub(super) fn hit_test_control_point(
        lane: &AutomationLane,
        view: &AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> Option<(u32, CtrlEnd, f32, f32, f32, f32)> {
        use lumino_core::SegmentShape;
        let hit_sq = HIT_RADIUS * HIT_RADIUS;
        // (prev_tick, which, x1, y1, x2, y2, dist_sq)
        let mut best: Option<(u32, CtrlEnd, f32, f32, f32, f32, f32)> = None;
        for i in 1..lane.events.len() {
            let prev = &lane.events[i - 1];
            let cur = &lane.events[i];
            let SegmentShape::Curve { x1, y1, x2, y2 } = prev.shape else {
                continue;
            };
            if prev.shape.is_linear() {
                continue;
            }
            let px0 = view.tick_to_x(prev.tick);
            let py0 = view.value_to_y(prev.value as f32, max_val);
            let px3 = view.tick_to_x(cur.tick);
            let py3 = view.value_to_y(cur.value as f32, max_val);
            // 两个控制点屏幕坐标（偏移量 *4 放大：P1 相对 P0，P2 相对 P3）
            let c1x = px0 + (px3 - px0) * x1 * SegmentShape::SCALE;
            let c1y = py0 + (py3 - py0) * y1 * SegmentShape::SCALE;
            let c2x = px3 + (px3 - px0) * x2 * SegmentShape::SCALE;
            let c2y = py3 + (py3 - py0) * y2 * SegmentShape::SCALE;
            let d1 = (c1x - cursor_pos.x).powi(2) + (c1y - cursor_pos.y).powi(2);
            let d2 = (c2x - cursor_pos.x).powi(2) + (c2y - cursor_pos.y).powi(2);
            if d1 <= hit_sq
                && best
                    .as_ref()
                    .map(|(_, _, _, _, _, _, d)| d1 < *d)
                    .unwrap_or(true)
            {
                best = Some((prev.tick, CtrlEnd::Out, x1, y1, x2, y2, d1));
            }
            if d2 <= hit_sq
                && best
                    .as_ref()
                    .map(|(_, _, _, _, _, _, d)| d2 < *d)
                    .unwrap_or(true)
            {
                best = Some((prev.tick, CtrlEnd::In, x1, y1, x2, y2, d2));
            }
        }
        best.map(|(t, w, x1, y1, x2, y2, _)| (t, w, x1, y1, x2, y2))
    }

    /// 从鼠标屏幕位置反推 Curve 段某一端控制点的偏移量 (x, y) ∈ [-0.5, 0.5]。
    pub(super) fn compute_ctrl_from_mouse(
        lane: &AutomationLane,
        prev_tick: u32,
        which: CtrlEnd,
        mouse: Point,
        view: &AutomationViewParams,
        max_val: f32,
    ) -> Option<(f32, f32)> {
        use lumino_core::SegmentShape;
        let prev_idx = lane.events.iter().position(|e| e.tick == prev_tick)?;
        let prev = &lane.events[prev_idx];
        let next = lane.events.get(prev_idx + 1)?;
        let px0 = view.tick_to_x(prev.tick);
        let py0 = view.value_to_y(prev.value as f32, max_val);
        let px3 = view.tick_to_x(next.tick);
        let py3 = view.value_to_y(next.value as f32, max_val);
        let dx = px3 - px0;
        let dy = py3 - py0;
        // 参考点：Out 用 P0，In 用 P3
        let (rx, ry) = match which {
            CtrlEnd::Out => (px0, py0),
            CtrlEnd::In => (px3, py3),
        };
        // x 方向 clamp 到 CSS 单调区间
        let x_range = match which {
            CtrlEnd::Out => (0.0, 0.25),
            CtrlEnd::In => (-0.25, 0.0),
        };
        let new_x = if dx.abs() < 1e-3 {
            0.0
        } else {
            ((mouse.x - rx) / dx / SegmentShape::SCALE).clamp(x_range.0, x_range.1)
        };
        let new_y = if dy.abs() < 1e-3 {
            0.0
        } else {
            ((mouse.y - ry) / dy / SegmentShape::SCALE).clamp(-0.5, 0.5)
        };
        Some((new_x, new_y))
    }

    /// 把拖拽出的控制点 (x, y) 按端别合并进 prev_tick 事件的 shape。
    pub(super) fn merge_ctrl_shape(
        lane: &AutomationLane,
        prev_tick: u32,
        which: CtrlEnd,
        new_ctrl: (f32, f32),
    ) -> lumino_core::SegmentShape {
        use lumino_core::SegmentShape;
        lane.events
            .iter()
            .find(|e| e.tick == prev_tick)
            .map(|e| match e.shape {
                SegmentShape::Curve { x1, y1, x2, y2 } => match which {
                    CtrlEnd::Out => SegmentShape::Curve {
                        x1: new_ctrl.0,
                        y1: new_ctrl.1,
                        x2,
                        y2,
                    },
                    CtrlEnd::In => SegmentShape::Curve {
                        x1,
                        y1,
                        x2: new_ctrl.0,
                        y2: new_ctrl.1,
                    },
                },
                SegmentShape::Step => SegmentShape::Step,
            })
            .unwrap_or(SegmentShape::Step)
    }
}
