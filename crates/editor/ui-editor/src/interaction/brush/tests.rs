//! 画刷矢量笔画交互与提交测试
//!
//! 覆盖卡面验收要点：断墨（快速拖动连续）、拖动期间不写 document、
//! √ 一次历史记录、多笔画共存、层音轨分配（含基准轨语义）、Conductor 拦截、
//! 命名/颜色规则、批量归并路径。

use super::*;
use crate::message::{EditorAction, Point2};
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_editor_state::brush_tool::cov;

/// 构造画刷编辑器：2 轨（Conductor + 1 普通轨），当前轨 = 1，精度 240
fn brush_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 直接种一笔待确认笔画（等价于「按下 → 拖动 → 松手」的落点结果）
fn seed_stroke(editor: &mut Editor, points: &[(f32, f32)]) {
    let base = editor.editor_state.data.current_track;
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    editor.editor_state.brush_tool.begin_stroke(*first, base);
    // 与交互路径一致：新笔画占一步笔画历史
    editor.editor_state.brush_tool.push_path_history();
    for point in rest {
        editor.editor_state.brush_tool.push_point(*point);
    }
    editor.editor_state.brush_tool.finish_stroke();
    editor.editor_state.brush_tool.update_top_path_history();
}

/// 某轨全部音符 `(tick, key)`（按文档顺序）
fn notes_of(editor: &Editor, track: usize) -> Vec<(f32, u8)> {
    editor
        .editor_state
        .data
        .track_notes(track)
        .iter()
        .map(|n| (n.start_tick as f32, n.key))
        .collect()
}

// ── 断墨（快速拖动） ─────────────────────────

// ── 待确认笔画的撤销/重做（走真实 EditorAction 链路） ─────────────────────────

/// 画一笔（真实链路 Pressed → Moved… → Released）
///
/// key 必须落在画布内（`handle_action` 会经 `is_inside_canvas` 守卫），
/// 故调用方传 `VISIBLE_KEY` 附近的 key。
fn stroke_via_actions(editor: &mut Editor, points: &[(f32, f32)]) {
    let start = editor.line_pos_screen_pos(points[0]);
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(start.x, start.y),
        shift: false,
        ctrl: false,
    });
    for point in points.iter().skip(1) {
        let screen = editor.line_pos_screen_pos(*point);
        editor.handle_action(EditorAction::Moved(Point2::new(screen.x, screen.y)));
    }
    editor.handle_action(EditorAction::Released);
}

/// 画布内可见的 key（画布 600 高 / zoom_y 20：key 100 位于 y=564）
const VISIBLE_KEY: f32 = 100.0;

mod basic;
mod interaction;
mod undo;
