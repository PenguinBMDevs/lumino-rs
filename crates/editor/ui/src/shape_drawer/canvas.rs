//! 形状抽屉 Canvas：工具栏、六工具交互、缩放/平移与 stroke 绘制。
//!
//! 交互直接修改共享的 `Rc<RefCell<DrawerDoc>>`，不经过全局 Message 路由；
//! 这样 B1 的画板可以独立演进，B2 再通过 Root/动作接入库与工程插入。

use std::cell::RefCell;
use std::rc::Rc;

use iced_core::keyboard;
use iced_core::mouse;
use iced_core::{Point, Rectangle, keyboard::key};
use iced_widget::canvas::{self, Frame, Geometry, Program};

use crate::shape_drawer::doc::{Draft, DrawerDoc, DrawerInteraction, DrawerTool};
use crate::{Message, Renderer, Theme};

use draw::{
    TOOLBAR_WIDTH, ToolbarAction, draw_canvas, draw_stroke, draw_toolbar, local_to_unit,
    toolbar_hit, unit_side,
};

mod draw;

/// Canvas Program；doc/interaction 由 Root 持有，窗口重建不丢。
#[derive(Debug, Clone)]
pub struct ShapeDrawerCanvas {
    /// 文档
    pub doc: Rc<RefCell<DrawerDoc>>,
    /// 拖拽草稿
    pub interaction: Rc<RefCell<DrawerInteraction>>,
    /// 库目录（占位显示）
    pub shapes_dir: String,
}

impl Program<Message, Theme, Renderer> for ShapeDrawerCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position()?;
                if !bounds.contains(position) {
                    return None;
                }
                let local = Point::new(position.x - bounds.x, position.y - bounds.y);
                if local.x < TOOLBAR_WIDTH {
                    self.toolbar_press(local);
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                let view = self.doc.borrow().view;
                let unit = local_to_unit(local, bounds, view);
                {
                    let mut doc = self.doc.borrow_mut();
                    if doc.area_mode {
                        let colour = doc.area_colour;
                        doc.paint_area(unit.0, unit.1, colour);
                        return Some(canvas::Action::request_redraw().and_capture());
                    }
                }
                let tool = self.doc.borrow().tool;
                if tool == DrawerTool::Select {
                    let tolerance = 0.035 / view.zoom.max(0.25);
                    self.doc.borrow_mut().select_at(unit, tolerance);
                } else if let Some(draft) = Draft::begin(tool, unit) {
                    self.interaction.borrow_mut().draft = Some(draft);
                }
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let position = cursor.position()?;
                if !bounds.contains(position) {
                    return None;
                }
                let local = Point::new(position.x - bounds.x, position.y - bounds.y);
                let mut interaction = self.interaction.borrow_mut();
                let shift = interaction.shift;
                if let Some(draft) = interaction.draft.as_mut() {
                    let view = self.doc.borrow().view;
                    let unit = local_to_unit(local, bounds, view);
                    draft.update_with_shift(unit, shift);
                    return Some(canvas::Action::request_redraw());
                }
                if let Some(start) = interaction.pan_start {
                    let mut doc = self.doc.borrow_mut();
                    let side = unit_side(bounds, doc.view);
                    doc.view.pan.0 += (local.x - start.0) / side;
                    doc.view.pan.1 += (local.y - start.1) / side;
                    interaction.pan_start = Some((local.x, local.y));
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let position = cursor.position()?;
                if !bounds.contains(position) {
                    return None;
                }
                self.interaction.borrow_mut().pan_start =
                    Some((position.x - bounds.x, position.y - bounds.y));
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                self.interaction.borrow_mut().pan_start = None;
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut interaction = self.interaction.borrow_mut();
                if let Some(draft) = interaction.draft.take() {
                    let stroke = draft.to_stroke();
                    drop(interaction);
                    self.doc.borrow_mut().commit_draft(stroke);
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let position = cursor.position()?;
                if !bounds.contains(position) {
                    return None;
                }
                let factor = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => {
                        if *y > 0.0 {
                            1.1
                        } else {
                            1.0 / 1.1
                        }
                    }
                    mouse::ScrollDelta::Pixels { y, .. } => {
                        if *y > 0.0 {
                            1.05
                        } else {
                            1.0 / 1.05
                        }
                    }
                };
                self.doc.borrow_mut().view.zoom_by(factor);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                self.interaction.borrow_mut().shift = modifiers.shift();
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Window(iced_core::window::Event::Unfocused) => {
                let mut interaction = self.interaction.borrow_mut();
                interaction.draft = None;
                interaction.pan_start = None;
                interaction.shift = false;
                Some(canvas::Action::capture())
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key::Key::Named(key::Named::Delete),
                ..
            }) => {
                self.doc.borrow_mut().delete_selected();
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key::Key::Character(character),
                modifiers,
                ..
            }) if modifiers.control() => {
                match character.as_str() {
                    "z" | "Z" => {
                        self.doc.borrow_mut().undo();
                    }
                    "y" | "Y" => {
                        self.doc.borrow_mut().redo();
                    }
                    _ => return None,
                }
                Some(canvas::Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), palette.background.base.color);
        frame.fill_rectangle(
            Point::ORIGIN,
            iced_core::Size::new(TOOLBAR_WIDTH, bounds.height),
            palette.background.weak.color,
        );

        let doc = self.doc.borrow();
        draw_toolbar(&mut frame, &doc, theme);
        let area = doc.area_map_cached();
        draw_canvas(
            &mut frame,
            &doc,
            bounds,
            theme,
            &self.shapes_dir,
            area.as_deref(),
        );
        if let Some(draft) = self.interaction.borrow().draft.as_ref()
            && let Some(stroke) = draft.to_stroke()
        {
            draw_stroke(
                &mut frame,
                &stroke,
                bounds,
                doc.view,
                palette.background.base.text,
                true,
            );
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        drawer_cursor(bounds, cursor.position(), TOOLBAR_WIDTH)
    }
}

/// 抽屉光标：仅画板区域（Canvas 内、工具条右侧）返回十字，其余默认。
///
/// iced 的 `mouse_interaction` 可能在 cursor 离开 Canvas bounds 后仍被沿用，
/// 因此必须显式判断 bounds 与工具条，不能无条件返回 Crosshair。
pub(crate) fn drawer_cursor(
    bounds: Rectangle,
    cursor: Option<Point>,
    toolbar_width: f32,
) -> mouse::Interaction {
    let Some(position) = cursor else {
        return mouse::Interaction::Idle;
    };
    if !bounds.contains(position) {
        return mouse::Interaction::Idle;
    }
    if position.x - bounds.x <= toolbar_width {
        return mouse::Interaction::Idle;
    }
    mouse::Interaction::Crosshair
}

impl ShapeDrawerCanvas {
    fn toolbar_press(&self, local: Point) {
        let Some(action) = toolbar_hit(local) else {
            return;
        };
        if matches!(action, ToolbarAction::Tool(_)) {
            self.interaction.borrow_mut().draft = None;
        }
        let mut doc = self.doc.borrow_mut();
        match action {
            ToolbarAction::Tool(tool) => doc.set_tool(tool),
            ToolbarAction::Undo => {
                doc.undo();
            }
            ToolbarAction::Redo => {
                doc.redo();
            }
            ToolbarAction::Delete => {
                doc.delete_selected();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(10.0, 20.0), iced_core::Size::new(400.0, 300.0))
    }

    #[test]
    fn test_drawer_cursor_inside_board_is_crosshair() {
        assert_eq!(
            drawer_cursor(bounds(), Some(Point::new(200.0, 150.0)), TOOLBAR_WIDTH),
            mouse::Interaction::Crosshair
        );
    }

    #[test]
    fn test_drawer_cursor_toolbar_is_default() {
        assert_eq!(
            drawer_cursor(bounds(), Some(Point::new(30.0, 150.0)), TOOLBAR_WIDTH),
            mouse::Interaction::Idle
        );
    }

    #[test]
    fn test_drawer_cursor_outside_or_missing_is_default() {
        assert_eq!(
            drawer_cursor(bounds(), Some(Point::new(5.0, 5.0)), TOOLBAR_WIDTH),
            mouse::Interaction::Idle
        );
        assert_eq!(
            drawer_cursor(bounds(), None, TOOLBAR_WIDTH),
            mouse::Interaction::Idle
        );
    }
}
