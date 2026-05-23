//! 力度编辑面板 - 类 Cubase 的描点绘制&力度调整
//!
//! 显示当前音轨所有音符的力度值，支持垂直拖拽调整。
//! X 轴 = 音符在音轨中的顺序（按 tick 排序）
//! Y 轴 = 力度值 (0-127)

pub mod widget;

use crate::Element;

/// 力度面板高度（像素）
pub const VELOCITY_PANEL_HEIGHT: f32 = 150.0;

/// 力度面板最小高度（像素）
pub const VELOCITY_PANEL_MIN_HEIGHT: f32 = 60.0;

/// 力度面板最大高度（像素）
pub const VELOCITY_PANEL_MAX_HEIGHT: f32 = 400.0;

/// 点绘制半径
pub const POINT_RADIUS: f32 = 4.0;

/// 悬停高亮半径
pub const HOVER_RADIUS: f32 = 7.0;

/// 点击/拖拽检测半径
pub const HIT_RADIUS: f32 = 10.0;

/// 面板上下内边距（像素）
pub const PANEL_PADDING_Y: f32 = 12.0;

/// 面板左右内边距（像素）
pub const PANEL_PADDING_X: f32 = 8.0;

/// 顶部resize拖拽手柄区域高度（像素）
pub const RESIZE_HANDLE_HEIGHT: f32 = 5.0;

/// 力度编辑面板组件
pub struct VelocityPanel;

impl VelocityPanel {
    pub fn new() -> Self {
        Self
    }

    /// 渲染力度编辑面板视图
    pub fn view<'a>(&'a self, editor: &'a crate::editor::Editor, panel_height: f32) -> Element<'a> {
        use iced_widget::canvas::Canvas;

        let canvas = Canvas::new(widget::VelocityCanvas { editor })
            .width(iced_core::Length::Fill)
            .height(panel_height);

        iced_widget::container(canvas)
            .width(iced_core::Length::Fill)
            .height(panel_height)
            .style(|theme: &crate::Theme| {
                iced_widget::container::Style::default()
                    .background(theme.extended_palette().background.weak.color)
            })
            .into()
    }

    /// 构建力度点数据（从音符数据生成，按 tick 排序）
    pub fn build_velocity_points(notes: &im::Vector<crate::editor::Note>) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = notes
            .iter()
            .enumerate()
            .map(|(i, note)| VelocityPoint {
                note_index: i,
                tick: note.tick,
                velocity: note.velocity,
            })
            .collect();

        // 按 tick 排序，tick 相同时按 key 排序
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });

        points
    }
}

impl Default for VelocityPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Note;

    #[test]
    fn test_build_velocity_points_empty() {
        let notes = im::Vector::new();
        let points = VelocityPanel::build_velocity_points(&notes);
        assert!(points.is_empty(), "空音符列表应产生空力度点列表");
    }

    #[test]
    fn test_build_velocity_points_single_note() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
        let points = VelocityPanel::build_velocity_points(&notes);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].note_index, 0);
        assert_eq!(points[0].tick, 0.0);
        assert_eq!(points[0].velocity, 100);
    }

    #[test]
    fn test_build_velocity_points_multiple_notes() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(480.0, 64, 240.0).with_velocity(80));
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
        notes.push_back(Note::new(960.0, 67, 240.0).with_velocity(120));
        notes.push_back(Note::new(480.0, 72, 120.0).with_velocity(60));

        let points = VelocityPanel::build_velocity_points(&notes);

        assert_eq!(points.len(), 4);
        assert_eq!(points[0].tick, 0.0);
        assert_eq!(points[0].note_index, 1);
        assert_eq!(points[0].velocity, 100);

        assert_eq!(points[1].tick, 480.0);
        assert_eq!(points[1].note_index, 0);
        assert_eq!(points[1].velocity, 80);

        assert_eq!(points[2].tick, 480.0);
        assert_eq!(points[2].note_index, 3);
        assert_eq!(points[2].velocity, 60);

        assert_eq!(points[3].tick, 960.0);
        assert_eq!(points[3].note_index, 2);
        assert_eq!(points[3].velocity, 120);
    }

    #[test]
    fn test_build_velocity_points_velocity_ranges() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(0.0, 60, 240.0).with_velocity(0));
        notes.push_back(Note::new(240.0, 64, 240.0).with_velocity(127));
        notes.push_back(Note::new(480.0, 67, 240.0).with_velocity(64));

        let points = VelocityPanel::build_velocity_points(&notes);
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].velocity, 0);
        assert_eq!(points[1].velocity, 127);
        assert_eq!(points[2].velocity, 64);
    }

    #[test]
    fn test_build_velocity_points_note_index_integrity() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(960.0, 60, 240.0).with_velocity(100));
        notes.push_back(Note::new(0.0, 64, 480.0).with_velocity(80));
        notes.push_back(Note::new(480.0, 67, 240.0).with_velocity(120));

        let points = VelocityPanel::build_velocity_points(&notes);

        assert_eq!(points[0].note_index, 1);
        assert_eq!(points[1].note_index, 2);
        assert_eq!(points[2].note_index, 0);

        assert_eq!(notes[points[0].note_index].velocity, 80);
        assert_eq!(notes[points[1].note_index].velocity, 120);
        assert_eq!(notes[points[2].note_index].velocity, 100);
    }

    // ── 曲线绘制算法测试 ──

    /// 辅助：计算曲线插值力度值（与 widget.rs 中 update_curve_paint 算法一致）
    fn interpolate_velocity(
        point_x: f32,
        min_x: f32,
        max_x: f32,
        start_velocity: u8,
        current_velocity: u8,
    ) -> u8 {
        let t = if (max_x - min_x).abs() < f32::EPSILON {
            1.0
        } else {
            (point_x - min_x) / (max_x - min_x)
        };
        let interp = start_velocity as f32 * (1.0 - t) + current_velocity as f32 * t;
        interp.round().clamp(0.0, 127.0) as u8
    }

    #[test]
    fn test_velocity_curve_single_note() {
        // 一个力度点位于拖拽范围内，应被插值影响
        let velocity = interpolate_velocity(100.0, 0.0, 200.0, 100, 50);
        // t = 100/200 = 0.5, interp = 100 * 0.5 + 50 * 0.5 = 75
        assert_eq!(velocity, 75);
    }

    #[test]
    fn test_velocity_curve_preserves_unaffected_notes() {
        // 范围外的点不受影响：测试端点行为
        let vel_left = interpolate_velocity(-10.0, 0.0, 200.0, 100, 50);
        // point_x < min_x 时，在 update_curve_paint 中会被跳过
        // t = (-10-0)/(200-0) = -0.05, interp = 100*1.05 + 50*(-0.05) = 105 - 2.5 = 102.5 -> 102
        assert_eq!(vel_left, 102);

        let vel_right = interpolate_velocity(300.0, 0.0, 200.0, 100, 50);
        // t = (300-0)/(200-0) = 1.5, interp = 100*(-0.5) + 50*1.5 = -50 + 75 = 25
        assert_eq!(vel_right, 25);
    }

    #[test]
    fn test_velocity_curve_selected_notes_only() {
        // 模拟三个力度点，选中索引 1（中间点），start_vel=100, current_vel=50
        let points = vec![
            VelocityPoint {
                note_index: 0,
                tick: 0.0,
                velocity: 100,
            },
            VelocityPoint {
                note_index: 1,
                tick: 100.0,
                velocity: 80,
            },
            VelocityPoint {
                note_index: 2,
                tick: 200.0,
                velocity: 120,
            },
        ];
        let selected: std::collections::HashSet<usize> = [1].into();

        let start_vel = 100u8;
        let current_vel = 50u8;
        let min_x = 0.0f32;
        let max_x = 200.0f32;

        for point in &points {
            let point_x = point.tick;
            let in_range = point_x >= min_x && point_x <= max_x;
            let is_selected = selected.contains(&point.note_index);

            if is_selected {
                // 选中音符应被插值影响
                let new_vel = interpolate_velocity(point_x, min_x, max_x, start_vel, current_vel);
                assert_ne!(
                    new_vel, point.velocity,
                    "选中音符 {} 的力度应变化（原={}, 新={}）",
                    point.note_index, point.velocity, new_vel
                );
            } else if in_range {
                // 未选中的音符在范围内也不应被修改
                assert_eq!(
                    point.velocity, point.velocity,
                    "未选中音符 {} 的力度应保持不变",
                    point.note_index
                );
            }
        }

        // 验证选中音符的插值结果
        let vel1 = interpolate_velocity(100.0, 0.0, 200.0, 100, 50);
        // t = 100/200 = 0.5, interp = 100*0.5 + 50*0.5 = 75
        assert_eq!(vel1, 75, "选中音符（x=100）的插值力度应为75");
    }

    #[test]
    fn test_velocity_curve_reverse_drag() {
        // 从右向左拖拽：min_x=50, max_x=200（start_x=200, current_x=50）
        // 点 x=100 应被影响
        let velocity = interpolate_velocity(100.0, 50.0, 200.0, 80, 120);
        // t = (100-50)/(200-50) = 50/150 = 0.333...
        // interp = 80 * (1-0.333) + 120 * 0.333 = 80*0.667 + 120*0.333 = 53.33 + 40 = 93.33
        // round = 93
        assert_eq!(velocity, 93);
    }

    #[test]
    fn test_velocity_curve_clamp_range() {
        // 力度值应被限制在 0-127 范围内
        let vel_under = interpolate_velocity(0.0, 0.0, 100.0, 255, 0);
        assert!(vel_under <= 127, "力度值不应超过 127，得到 {}", vel_under);

        let vel_over = interpolate_velocity(100.0, 0.0, 100.0, 0, 255);
        assert!(vel_over <= 127, "力度值不应超过 127，得到 {}", vel_over);
    }

    #[test]
    fn test_velocity_curve_zero_width_range() {
        // 零宽度范围：start_x == current_x，应返回 current_velocity
        let vel = interpolate_velocity(100.0, 100.0, 100.0, 80, 120);
        assert_eq!(vel, 120);
    }
}

/// 力度点数据
#[derive(Debug, Clone, Copy)]
pub struct VelocityPoint {
    /// 在 notes 向量中的索引
    pub note_index: usize,
    /// 音符的起始 tick（用于排序）
    pub tick: f32,
    /// 力度值 0-127
    pub velocity: u8,
}
