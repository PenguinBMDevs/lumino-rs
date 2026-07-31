//! 布局相关方法：坐标转换、命中测试、resize 区域检测

use iced_core::{Point, Size};
use lumino_core::{AutomationLane, AutomationTarget};
use lumino_gfx::automation::AutomationViewParams;

use super::super::super::{
    HIT_RADIUS, PANEL_PADDING_Y, RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT, VelocityPoint,
};
use crate::editor_state::ViewState;
use crate::velocity::EditMode;
use crate::velocity::widget::TempoPoint;

impl super::super::VelocityCanvas<'_> {
    /// 获取所有力度点
    ///
    /// **性能优化**：NoteStore 启用时走 `build_velocity_points_from_store`，
    /// 10M+ 音符场景下避免全部 Note clone 开销（百毫秒级）。
    pub(super) fn points(&self) -> Vec<VelocityPoint> {
        let editor_data = &self.editor.editor_state.data;
        if editor_data.is_note_store_enabled() {
            super::super::super::VelocityPanel::build_velocity_points_from_store(
                &editor_data.note_store,
            )
        } else {
            super::super::super::VelocityPanel::build_velocity_points(&editor_data.notes)
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
        let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        let point_y = Self::velocity_to_y(point.velocity, bounds_height);
        Point::new(point_x, point_y)
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
        let editor_data = &self.editor.editor_state.data;
        let track = editor_data.current_track as u16;
        editor_data
            .find_automation_lane(track, &target)
            .and_then(|idx| editor_data.automation_lanes.get(idx).map(|a| &**a))
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
            let evt_x = view.tick_to_x(evt.tick);
            let evt_y = view.value_to_y(evt.value as f32, max_val);
            let dx = cursor_pos.x - evt_x;
            let dy = cursor_pos.y - evt_y;
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
}
