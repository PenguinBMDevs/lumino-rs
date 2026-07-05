//! 布局相关方法：坐标转换、命中测试、resize 区域检测

use iced_core::Point;

use super::super::super::{HIT_RADIUS, PANEL_PADDING_Y, RESIZE_HANDLE_HEIGHT, VelocityPoint};
use crate::editor::editor_state::ViewState;

impl super::super::VelocityCanvas<'_> {
    /// 获取所有力度点
    pub(super) fn points(&self) -> Vec<VelocityPoint> {
        let notes = &self.editor.editor_state.data.notes;
        super::super::super::VelocityPanel::build_velocity_points(notes)
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
}
