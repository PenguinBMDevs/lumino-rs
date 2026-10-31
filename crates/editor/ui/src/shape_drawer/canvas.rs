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

use crate::shape_drawer::doc::{
    AnchorDrag, AnchorPart, Draft, DrawerDoc, DrawerInteraction, DrawerTool, DrawerView,
};
use crate::{Message, Renderer, Theme};

use draw::{
    TOOLBAR_WIDTH, ToolbarAction, draw_canvas, draw_stroke, draw_toolbar, local_to_unit,
    toolbar_hit, unit_side, unit_to_local,
};

mod draw;

use lumino_core::shape::{AnchorBend, StrokeKind};

/// 命中选中 stroke 的锚点/控制柄（屏幕像素容差）
fn hit_anchor(
    doc: &DrawerDoc,
    local: Point,
    bounds: Rectangle,
    view: DrawerView,
) -> Option<(usize, usize, AnchorPart)> {
    const TOLERANCE: f32 = 6.0;
    let mut best: Option<(f32, usize, usize, AnchorPart)> = None;
    for &stroke_index in &doc.selection {
        let Some(stroke) = doc.strokes.get(stroke_index) else {
            continue;
        };
        for (anchor_index, anchor) in stroke.anchors.iter().enumerate() {
            let pos = unit_to_local(anchor.pos, bounds, view);
            let distance = ((local.x - pos.x).powi(2) + (local.y - pos.y).powi(2)).sqrt();
            if distance <= TOLERANCE && best.is_none_or(|(current, ..)| distance < current) {
                best = Some((distance, stroke_index, anchor_index, AnchorPart::Pos));
            }
        }
    }
    if best.is_some() {
        return best.map(|(_, s, a, part)| (s, a, part));
    }
    for &stroke_index in &doc.selection {
        let Some(stroke) = doc.strokes.get(stroke_index) else {
            continue;
        };
        if stroke.kind != StrokeKind::Curve {
            continue;
        }
        for (anchor_index, anchor) in stroke.anchors.iter().enumerate() {
            if anchor.bend == AnchorBend::Corner {
                continue;
            }
            for part in [AnchorPart::In, AnchorPart::Out] {
                let handle = match part {
                    AnchorPart::In => anchor.in_handle_abs(),
                    AnchorPart::Out => anchor.out_handle_abs(),
                    AnchorPart::Pos => continue,
                };
                if handle == anchor.pos {
                    continue;
                }
                let screen = unit_to_local(handle, bounds, view);
                let distance = ((local.x - screen.x).powi(2) + (local.y - screen.y).powi(2)).sqrt();
                if distance <= TOLERANCE && best.is_none_or(|(current, ..)| distance < current) {
                    best = Some((distance, stroke_index, anchor_index, part));
                }
            }
        }
    }
    best.map(|(_, s, a, part)| (s, a, part))
}

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
                    let mut doc = self.doc.borrow_mut();
                    let tolerance = 0.035 / view.zoom.max(0.25);
                    let hit = hit_anchor(&doc, local, bounds, view);
                    if let Some((stroke, anchor, part)) = hit {
                        doc.selected_anchor = Some((stroke, anchor));
                        self.interaction.borrow_mut().anchor_drag = Some(AnchorDrag {
                            stroke,
                            anchor,
                            part,
                            started: false,
                        });
                    } else {
                        doc.select_at(unit, tolerance);
                        self.interaction.borrow_mut().anchor_drag = None;
                    }
                } else if matches!(tool, DrawerTool::Curve | DrawerTool::Arc) {
                    let mut interaction = self.interaction.borrow_mut();
                    let committed = match interaction.draft.as_mut() {
                        Some(draft) => draft.click(unit),
                        None => {
                            interaction.draft = Draft::begin(tool, unit);
                            None
                        }
                    };
                    drop(interaction);
                    if let Some(stroke) = committed {
                        self.interaction.borrow_mut().draft = None;
                        self.doc.borrow_mut().commit_draft(Some(stroke));
                    }
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
                if let Some(mut drag) = interaction.anchor_drag {
                    let view = self.doc.borrow().view;
                    let unit = local_to_unit(local, bounds, view);
                    let changed = {
                        let mut doc = self.doc.borrow_mut();
                        doc.drag_anchor(&mut drag, unit, shift)
                    };
                    interaction.anchor_drag = Some(drag);
                    if changed {
                        return Some(canvas::Action::request_redraw());
                    }
                }
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
                if interaction.anchor_drag.take().is_some() {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if interaction
                    .draft
                    .as_ref()
                    .is_some_and(Draft::is_multi_stage)
                {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
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

    #[test]
    fn test_hit_anchor_prefers_anchor_over_handle() {
        use lumino_core::shape::{ShapeAnchor, Stroke, StrokeKind};
        let mut doc = DrawerDoc::default();
        doc.strokes.push(Stroke {
            kind: StrokeKind::Curve,
            anchors: vec![ShapeAnchor::new((0.5, 0.5)), ShapeAnchor::new((0.8, 0.5))],
            rect: None,
        });
        doc.selection.insert(0);
        let bounds = Rectangle::new(Point::new(0.0, 0.0), iced_core::Size::new(400.0, 400.0));
        let view = DrawerView::default();
        let position = unit_to_local((0.5, 0.5), bounds, view);
        assert_eq!(
            hit_anchor(&doc, position, bounds, view),
            Some((0, 0, AnchorPart::Pos))
        );
    }
}
