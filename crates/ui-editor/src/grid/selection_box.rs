//! 选择框渲染

use crate::Editor;
use crate::grid::utils::{clip_rect, content_bounds};
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::{
    SELECTION_BOX_FILL_COLOR, SELECTION_BOX_STROKE_COLOR, SELECTION_BOX_STROKE_WIDTH,
};

/// 绘制单个框选框（填充 + 描边）
///
/// 所有框选框统一样式：灰色边框（3px）+ 比边框浅一点的灰色半透明填充。
/// 绘制发生在 canvas 层，音符由 GPU NoteInstance 通道叠加在 canvas 之上，
/// 因此框选框与填充永远位于音符下方，不会遮挡音符显示。
fn draw_box(frame: &mut canvas::Frame<Renderer>, rect: Rectangle) {
    let path = Path::rectangle(rect.position(), rect.size());
    frame.fill(&path, SELECTION_BOX_FILL_COLOR);
    let stroke = Stroke::default()
        .with_width(SELECTION_BOX_STROKE_WIDTH)
        .with_color(SELECTION_BOX_STROKE_COLOR);
    frame.stroke(&path, stroke);
}

/// 绘制选择框
///
/// 两种情况会绘制选择框：
/// 1. 正在拖拽框选时（`EditState::Selecting`）——绘制半透明填充的选择框
/// 2. 有已选中的音符时——绘制围绕所有选中音符的方形边界框
///
/// 两类选框均裁剪到卷帘内容区（键盘列右侧、标尺下方）内绘制：
/// 框选/拖拽允许越过内容区边界（负 tick、键盘列上方），
/// 但选框不得显示到键盘列/标尺之上。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    _theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    crate::puffin_profiler::selection_box_draw();
    let content = content_bounds(editor);
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;

    // 情况 1：正在拖拽框选——绘制半透明填充的选择框
    // 根据框选框模式决定显示方式：
    // - Direct 模式：直接使用当前实际位置（无动画延迟）
    // - Spring 模式：优先使用动画显示位置（弹簧弹性效果）
    let selection_box =
        if editor.selection_box_mode() == lumino_core::storage::config::SelectionBoxMode::Direct {
            editor.get_selection_box()
        } else {
            editor
                .selection_box_anim
                .get()
                .map(|anim| (anim.start_pos, anim.current_pos))
                .or_else(|| editor.get_selection_box())
        };

    if let Some((start_pos, current_pos)) = selection_box {
        let min_x = start_pos.x.min(current_pos.x);
        let max_x = start_pos.x.max(current_pos.x);
        let min_y = start_pos.y.min(current_pos.y);
        let max_y = start_pos.y.max(current_pos.y);

        let rect = Rectangle::new(
            Point::new(min_x, min_y),
            Size::new((max_x - min_x).max(1.0), (max_y - min_y).max(1.0)),
        );
        if let Some(clipped) = clip_rect(rect, content) {
            draw_box(&mut frame, clipped);
            has_content = true;
        }
    }

    // 情况 2：有已选中的音符——绘制围绕所有选中音符的方形边界框。
    // 仅在非 Selecting 状态下绘制，避免与情况 1 的平滑框选框重叠导致两个不同样式框选框。
    // 拖动期间使用 ghost 位置，使选择框跟随被拖动的音符一起移动。
    //
    // 复制模式（pending_copy / DraggingSelectionCopy）：`get_selection_box_rects`
    // 只返回**副本框**（最新件框选）——原件不再框选（用户要求）。
    // 连续复制拖动中副本框覆盖「旧副本 ∪ 新副本」边界。
    //
    // 性能优化：
    // - 非 Selecting 状态，使用 `get_selection_box_rects()`（增量维护的 O(1) 缓存，
    //   或 ghost 路径的 O(N) 计算），消除 selection_box::draw 中重复的 O(N) ghost 路径。
    // - 2026-07-20 重构：消除与 note_ops::get_selection_box_bounds 重复的 ghost 逻辑。
    //
    // 修复：cached_selection_bounds 仅在 Selecting 状态下有效（由 update_selection 增量维护），
    // 非 Selecting 状态下必须使用 get_selection_box_rects()。此前渲染代码未检查 edit_state，
    // 导致 DraggingSelection/Idle 状态下使用了 stale 的 cached_selection_bounds，框选框不跟随
    // 音符拖动（缺少 ghost delta），且二次框选时位置/大小异常。
    let selected = &editor.editor_state.interaction.selected_notes;
    let has_selection =
        !selected.is_empty() || editor.editor_state.interaction.selection_bitset.is_some();
    if has_selection {
        puffin::profile_scope!("draw::selection_box_bbox");

        // Selecting 状态下，情况 1 的半透明填充框已提供拖拽视觉反馈，
        // 跳过情况 2 的选中音符边界框，避免两个不同样式框选框重叠显示。
        if matches!(
            editor.editor_state.interaction.edit_state,
            crate::EditState::Selecting { .. }
        ) {
            // 跳过：Selecting 状态下仅由情况 1 绘制平滑框选框
        } else {
            for (min_x, max_x, min_y, max_y) in editor.get_selection_box_rects() {
                let rect = Rectangle::new(
                    Point::new(min_x, min_y),
                    Size::new(max_x - min_x, max_y - min_y),
                );
                if let Some(clipped) = clip_rect(rect, content) {
                    draw_box(&mut frame, clipped);
                    has_content = true;
                }
            }
        }
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}
