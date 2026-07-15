//! 工程走带视图点击 Canvas —— 透明覆盖层，捕获点击事件以移动演奏指示线
//!
//! 工程走带视图的主体由 WGPU ArrangementRenderer 渲染，但 WGPU 层不处理鼠标事件。
//! 此 Canvas 作为透明覆盖层叠加在走带区域上方，捕获鼠标点击并转换为 tick 值，
//! 通过 EditorAction::Scrubbed 消息移动演奏指示线位置。
//!
//! 坐标转换：tick = (click_x + scroll_x) / zoom_x
//! 其中 click_x 是相对于走带区域左侧的屏幕坐标（不含音轨列表宽度）

use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{self, Frame, Geometry, Program};

use crate::arrangement::ArrangementViewport;
use lumino_ui_core::message::EditorAction;
use crate::{Message, Renderer, Theme};

/// 工程走带点击 Canvas
pub struct ArrangementClickCanvas {
    /// 视口引用（用于坐标转换）
    pub viewport: ArrangementViewport,
}

impl Program<Message, Theme, Renderer> for ArrangementClickCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(pos) = cursor.position()
        {
            // 点击位置相对于走带区域左侧的 X 坐标
            let local_x = pos.x - bounds.x;
            // 转换为 tick 值：tick = (screen_x + scroll_x) / zoom_x
            let ppu = self.viewport.zoom_x.max(0.001);
            let tick = (local_x + self.viewport.scroll_x) / ppu;
            let snapped_tick = tick.max(0.0);
            return Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Scrubbed { tick: snapped_tick },
            )));
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // 透明 Canvas —— 不绘制任何内容，让 WGPU 渲染可见
        let frame = Frame::new(renderer, bounds.size());
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        // 在走带区域显示十字光标，提示可点击
        mouse::Interaction::Crosshair
    }
}
