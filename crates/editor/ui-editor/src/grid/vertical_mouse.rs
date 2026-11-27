//! 纵向卷帘鼠标光标。
//!
//! 直接复用横向 `cursor::handle_at` 的完整命中链（I2M / 切割 / 编辑态 / 循环手柄 /
//! 选框 / 指示线 / hover_state），只把「局部坐标」来源换成调用方传入的 `position`。
//! 旧实现只覆盖区域着色、橡皮与曲线三类，纵向卷帘因此少了四类光标反馈。

use crate::Editor;
use lumino_ui_core::geom::{Point, mouse};

/// 光标形态：与横向网格同一套判定，局部坐标由调用方提供。
pub(crate) fn handle(editor: &Editor, position: Option<Point>) -> mouse::Interaction {
    let state = crate::grid::GridInteractionState {
        position,
        ..Default::default()
    };
    crate::grid::cursor::handle_at(editor, &state, position)
}
