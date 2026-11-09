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

use crate::shape_drawer::doc::{DrawerDoc, DrawerInteraction};
use crate::{Message, Renderer, Theme};

use draw::{TOOLBAR_WIDTH, draw_canvas, draw_stroke, draw_toolbar};

pub(crate) mod draw;
pub(crate) mod input;

/// 形状抽屉画布：持有文档与交互状态（`Program` 只做事件/绘制适配）。
pub struct ShapeDrawerCanvas {
    /// 共享文档（工具 / 选中 / 视图都在这里）。
    pub(crate) doc: Rc<RefCell<DrawerDoc>>,
    /// 瞬时交互（草稿 / 平移 / shift / 锚点拖拽）。
    pub(crate) interaction: Rc<RefCell<DrawerInteraction>>,
    /// 展示在画布底部的形状库目录。
    pub(crate) shapes_dir: String,
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
        use input as ev;
        // 事件逻辑已抽到 `canvas::input`，egui 画布复用同一套函数。
        let local = |cursor: mouse::Cursor| {
            let position = cursor.position()?;
            if !bounds.contains(position) {
                return None;
            }
            Some(Point::new(position.x - bounds.x, position.y - bounds.y))
        };
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = local(cursor)?;
                ev::press(&self.doc, &self.interaction, bounds, point);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let point = local(cursor)?;
                let shift = self.interaction.borrow().shift;
                ev::moved(&self.doc, &self.interaction, bounds, point, shift);
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let point = local(cursor)?;
                ev::middle_press(&self.interaction, point);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                ev::middle_release(&self.interaction);
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                ev::release(&self.doc, &self.interaction);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                local(cursor)?;
                match delta {
                    mouse::ScrollDelta::Lines { y, .. } => ev::scroll(&self.doc, true, *y > 0.0),
                    mouse::ScrollDelta::Pixels { y, .. } => ev::scroll(&self.doc, false, *y > 0.0),
                }
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                self.interaction.borrow_mut().shift = modifiers.shift();
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Window(iced_core::window::Event::Unfocused) => {
                ev::unfocused(&self.interaction);
                Some(canvas::Action::capture())
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key::Key::Named(key::Named::Delete),
                ..
            }) => {
                ev::delete(&self.doc);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key::Key::Character(character),
                modifiers,
                ..
            }) if modifiers.control() => {
                match character.as_str() {
                    "z" | "Z" => ev::undo(&self.doc),
                    "y" | "Y" => ev::redo(&self.doc),
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
