//! 钢琴卷帘视图的滚动与缩放消息处理。
//!
//! 从 `state_update.rs` 的穷尽分派表按业务对象拆出；语句与读写顺序逐行保持原样。

use crate::root::Root;

impl Root {
    /// 横向滚动条：纵向卷帘下滚动时间轴，否则滚动钢琴卷帘横向位置
    pub(super) fn scrollbar_x(&mut self, x: f32) -> bool {
        if self.editor.editor_state.is_vertical_roll {
            self.editor.set_vertical_time_scroll(x);
        } else {
            let v = &self.editor.editor_state.view;
            self.editor.set_scroll_x(
                x,
                v.keyboard_width,
                self.editor.editor_state.canvas.size_x,
                v.zoom_x,
            );
        }
        true
    }

    /// 纵向滚动条：纵向卷帘下滚动键盘，否则滚动钢琴卷帘纵向位置
    pub(super) fn scrollbar_y(&mut self, y: f32) -> bool {
        if self.editor.editor_state.is_vertical_roll {
            self.editor.set_vertical_keyboard_scroll(y);
        } else {
            self.editor.set_scroll_y(y);
        }
        true
    }

    /// 横向缩放：纵向卷帘下作用于时间轴，否则作用于钢琴卷帘
    pub(super) fn zoom_x(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        if self.editor.editor_state.is_vertical_roll {
            self.editor.set_vertical_time_zoom(zoom, fixed_ratio);
        } else {
            self.editor.set_zoom_x(zoom, fixed_ratio);
        }
        true
    }

    /// 纵向缩放：纵向卷帘下作用于键盘，否则作用于钢琴卷帘
    pub(super) fn zoom_y(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        if self.editor.editor_state.is_vertical_roll {
            self.editor.set_vertical_keyboard_zoom(zoom, fixed_ratio);
        } else {
            self.editor.set_zoom_y(zoom, fixed_ratio);
        }
        true
    }
}
