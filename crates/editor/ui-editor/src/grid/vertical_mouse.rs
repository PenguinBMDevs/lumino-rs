//! 纵向卷帘鼠标光标（从 `vertical_program.rs` 拆出，控制文件长度）

use crate::Editor;
use iced_core::{Point, mouse};
use lumino_message::Tool;

/// 光标形态：区域着色/橡皮/曲线命中 → 十字或 Pointer，其余 Idle
pub(crate) fn handle(editor: &Editor, position: Option<Point>) -> mouse::Interaction {
    let in_grid = position.is_some_and(|pos| {
        let view = &editor.editor_state.view;
        pos.x >= view.keyboard_width && pos.y >= view.ruler_height
    });
    if editor.area_paint_active() {
        return if in_grid {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        };
    }
    if matches!(editor.current_tool(), Tool::Eraser | Tool::DrawEraser) {
        return if in_grid {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        };
    }
    if editor.current_tool() == Tool::Curve {
        if let Some(local_pos) = position
            && editor.line_tool_hit_test(local_pos).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        return if in_grid {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::Idle
        };
    }
    mouse::Interaction::Idle
}
