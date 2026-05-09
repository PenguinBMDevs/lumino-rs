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

/// 每个力度点的水平宽度（像素）
pub const SLOT_WIDTH: f32 = 8.0;

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
        notes.push_back(Note::new(480.0, 64, 240.0).with_velocity(80)); // tick 480
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100)); // tick 0
        notes.push_back(Note::new(960.0, 67, 240.0).with_velocity(120)); // tick 960
        notes.push_back(Note::new(480.0, 72, 120.0).with_velocity(60)); // tick 480

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
        notes.push_back(Note::new(960.0, 60, 240.0).with_velocity(100)); // index 0
        notes.push_back(Note::new(0.0, 64, 480.0).with_velocity(80)); // index 1
        notes.push_back(Note::new(480.0, 67, 240.0).with_velocity(120)); // index 2

        let points = VelocityPanel::build_velocity_points(&notes);

        assert_eq!(points[0].note_index, 1);
        assert_eq!(points[1].note_index, 2);
        assert_eq!(points[2].note_index, 0);

        assert_eq!(notes[points[0].note_index].velocity, 80);
        assert_eq!(notes[points[1].note_index].velocity, 120);
        assert_eq!(notes[points[2].note_index].velocity, 100);
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
