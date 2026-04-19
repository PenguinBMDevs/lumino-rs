//! 力度编辑器UI渲染

use iced_core::{Font, Point, Rectangle, Size};
use iced_core::mouse::Cursor;
use iced_widget::canvas;
use iced_widget::canvas::{Frame, Path, Stroke, Text};

use crate::editor::velocity_editor::{VelocityEditor, VelocityEditorRenderer, VelocityEditState};
use crate::editor::note::Note;

/// 力度编辑器画布
pub struct VelocityEditorCanvas<'a> {
    pub editor: &'a VelocityEditor,
    pub notes: &'a [Note],
    pub selected_notes: &'a [usize],
    pub width: f32,
    pub renderer: VelocityEditorRenderer,
}

impl<'a> VelocityEditorCanvas<'a> {
    pub fn new(
        editor: &'a VelocityEditor,
        notes: &'a [Note],
        selected_notes: &'a [usize],
        width: f32,
        is_dark_theme: bool,
    ) -> Self {
        let renderer = if is_dark_theme {
            VelocityEditorRenderer::dark_theme()
        } else {
            VelocityEditorRenderer::default()
        };

        Self {
            editor,
            notes,
            selected_notes,
            width,
            renderer,
        }
    }
}

impl<'a> canvas::Program<crate::Message> for VelocityEditorCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced_widget::renderer::Renderer,
        _theme: &crate::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let draw_area = self.editor.draw_area(self.width);

        // 绘制背景
        frame.fill_rectangle(
            Point::new(0.0, 0.0),
            bounds.size(),
            self.renderer.background_color,
        );

        // 绘制网格线
        self.draw_grid_lines(&mut frame, &draw_area);

        // 绘制力度条和瞄点
        self.draw_velocity_bars(&mut frame, &draw_area);

        // 绘制绘制预览（如果有）
        self.draw_drawing_preview(&mut frame, &draw_area);

        vec![frame.into_geometry()]
    }
}

impl<'a> VelocityEditorCanvas<'a> {
    /// 绘制网格线
    fn draw_grid_lines(&self, frame: &mut Frame, draw_area: &Rectangle) {
        let stroke = Stroke::default()
            .with_color(self.renderer.grid_line_color)
            .with_width(1.0);

        // 127线
        let y_127 = self.editor.velocity_to_y(127, draw_area);
        frame.stroke(
            &Path::line(
                Point::new(draw_area.x, y_127),
                Point::new(draw_area.x + draw_area.width, y_127),
            ),
            stroke.clone(),
        );

        // 64线
        let y_64 = self.editor.velocity_to_y(64, draw_area);
        frame.stroke(
            &Path::line(
                Point::new(draw_area.x, y_64),
                Point::new(draw_area.x + draw_area.width, y_64),
            ),
            stroke.clone(),
        );

        // 0线
        let y_0 = self.editor.velocity_to_y(0, draw_area);
        frame.stroke(
            &Path::line(
                Point::new(draw_area.x, y_0),
                Point::new(draw_area.x + draw_area.width, y_0),
            ),
            stroke,
        );

        // 绘制刻度文字
        let text_color = self.renderer.text_color;

        frame.fill_text(Text {
            content: "127".to_string(),
            position: Point::new(16.0, y_127 - 6.0),
            color: text_color,
            size: 10.0.into(),
            font: Font::default(),
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Center.into(),
        });

        frame.fill_text(Text {
            content: "64".to_string(),
            position: Point::new(20.0, y_64 - 6.0),
            color: text_color,
            size: 10.0.into(),
            font: Font::default(),
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Center.into(),
        });

        frame.fill_text(Text {
            content: "0".to_string(),
            position: Point::new(24.0, y_0 - 6.0),
            color: text_color,
            size: 10.0.into(),
            font: Font::default(),
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Center.into(),
        });
    }

    /// 绘制力度条和瞄点
    fn draw_velocity_bars(&self, frame: &mut Frame, draw_area: &Rectangle) {
        let handles = self.editor.calculate_handles(self.notes, self.selected_notes, draw_area);

        for handle in &handles {
            let bar_x = handle.center.x - self.editor.bar_width / 2.0;
            let bar_y = handle.center.y;
            let bar_height = draw_area.y + draw_area.height - bar_y;

            // 绘制力度条
            frame.fill_rectangle(
                Point::new(bar_x, bar_y),
                Size::new(self.editor.bar_width, bar_height),
                self.renderer.bar_color,
            );

            // 如果选中，绘制光晕
            if handle.selected {
                let glow_radius = self.editor.handle_radius * 1.5;
                frame.fill(
                    &Path::circle(handle.center, glow_radius),
                    self.renderer.selected_glow_color,
                );
            }

            // 绘制瞄点
            frame.fill(
                &Path::circle(handle.center, self.editor.handle_radius),
                self.renderer.handle_color,
            );

            // 绘制瞄点边框
            frame.stroke(
                &Path::circle(handle.center, self.editor.handle_radius),
                Stroke::default()
                    .with_color(self.renderer.handle_stroke_color)
                    .with_width(2.0),
            );
        }
    }

    /// 绘制绘制预览
    fn draw_drawing_preview(&self, frame: &mut Frame, draw_area: &Rectangle) {
        match &self.editor.edit_state {
            VelocityEditState::Drawing { start_x, start_velocity, current_x, current_velocity } |
            VelocityEditState::DrawingLine { start_x, start_velocity, current_x, current_velocity } => {
                let start_y = self.editor.velocity_to_y(*start_velocity, draw_area);
                let current_y = self.editor.velocity_to_y(*current_velocity, draw_area);

                // 绘制绘制路径
                let path = Path::line(
                    Point::new(*start_x, start_y),
                    Point::new(*current_x, current_y),
                );

                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(self.renderer.handle_stroke_color)
                        .with_width(2.0),
                );

                // 绘制起点和终点瞄点
                frame.fill(&Path::circle(Point::new(*start_x, start_y), 4.0), self.renderer.handle_stroke_color);
                frame.fill(&Path::circle(Point::new(*current_x, current_y), 4.0), self.renderer.handle_stroke_color);
            }
            _ => {}
        }
    }
}

/// 创建力度编辑器视图
pub fn velocity_editor_view<'a>(
    editor: &'a VelocityEditor,
    notes: &'a [Note],
    selected_notes: &'a [usize],
    width: f32,
    is_dark_theme: bool,
) -> iced_widget::Canvas<VelocityEditorCanvas<'a>, crate::Message> {
    let canvas = VelocityEditorCanvas::new(editor, notes, selected_notes, width, is_dark_theme);
    iced_widget::Canvas::new(canvas)
        .width(width)
        .height(editor.height)
}
