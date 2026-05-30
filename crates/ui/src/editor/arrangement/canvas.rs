//! 音轨总览 Canvas —— 仅用于获取布局边界，实际渲染由 wgpu 完成

use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas::{Action, Event, Geometry, Program};

use crate::{Message, Renderer, Theme};

/// 音轨总览画布 —— 不绘制任何内容，仅同步 bounds 到 ArrangementView
pub struct ArrangementCanvas;

impl Program<Message, Theme, Renderer> for ArrangementCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        _event: &Event,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let offset = Point::new(bounds.x, bounds.y);
        let size = iced_core::Size::new(bounds.width, bounds.height);

        Some(Action::publish(Message::ArrangementCanvasBoundsChanged {
            offset,
            size,
        }))
    }

    fn draw(
        &self,
        _state: &(),
        _renderer: &Renderer,
        _theme: &Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // 不绘制任何内容 —— 实际渲染由 wgpu NoteRenderer 完成
        vec![]
    }
}
