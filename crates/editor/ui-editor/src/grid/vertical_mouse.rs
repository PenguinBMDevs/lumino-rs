//! 纵向卷帘鼠标光标（从 `vertical_program.rs` 拆出，控制文件长度）

use crate::Editor;
use iced_core::{Point, mouse};
use lumino_message::Tool;

/// 光标形态：区域着色/橡皮/曲线命中 → 十字或 Pointer，其余 Idle
pub(crate) fn handle(editor: &Editor, position: Option<Point>) -> mouse::Interaction {
    if editor.area_paint_active() {
        return mouse::Interaction::Crosshair;
    }
    if matches!(editor.current_tool(), Tool::Eraser | Tool::DrawEraser) {
        return mouse::Interaction::Crosshair;
    }
    if editor.current_tool() == Tool::Curve {
        if let Some(local_pos) = position
            && editor.line_tool_hit_test(local_pos).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        return mouse::Interaction::Crosshair;
    }
    mouse::Interaction::Idle
}
