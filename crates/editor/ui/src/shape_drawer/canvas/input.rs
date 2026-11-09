//! 抽屉画布的事件逻辑（与绘制后端无关）。
//!
//! iced 的 `Canvas` 与 egui 画布共用这里的函数：两边只负责把各自的输入事件
//! 归一化成「局部坐标 + 画布边界」，再调用同一套命中/草稿/拖动逻辑，
//! 保证移植后行为完全一致。坐标均为「画布局部坐标」（画布左上角为原点）。

use std::cell::RefCell;
use std::rc::Rc;

use iced_core::{Point, Rectangle};

use crate::shape_drawer::canvas::draw::{
    TOOLBAR_WIDTH, ToolbarAction, local_to_unit, toolbar_hit, unit_side, unit_to_local,
};
use crate::shape_drawer::doc::{
    AnchorDrag, AnchorPart, Draft, DrawerDoc, DrawerInteraction, DrawerTool,
};

use lumino_core::shape::{AnchorBend, StrokeKind};

/// 命中选中 stroke 的锚点 / 控制柄（屏幕像素容差）。
pub(crate) fn hit_anchor(
    doc: &DrawerDoc,
    local: Point,
    bounds: Rectangle,
    view: crate::shape_drawer::doc::DrawerView,
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
                let pos = unit_to_local(handle, bounds, view);
                let distance = ((local.x - pos.x).powi(2) + (local.y - pos.y).powi(2)).sqrt();
                if distance <= TOLERANCE && best.is_none_or(|(current, ..)| distance < current) {
                    best = Some((distance, stroke_index, anchor_index, part));
                }
            }
        }
    }
    best.map(|(_, s, a, part)| (s, a, part))
}

/// 左键按下（`local` = 画布局部坐标；`bounds` = 画布矩形）。
pub(crate) fn press(
    doc: &Rc<RefCell<DrawerDoc>>,
    interaction: &Rc<RefCell<DrawerInteraction>>,
    bounds: Rectangle,
    local: Point,
) {
    if local.x < TOOLBAR_WIDTH {
        toolbar_press(doc, interaction, local);
        return;
    }
    let view = doc.borrow().view;
    let unit = local_to_unit(local, bounds, view);
    {
        let mut doc_ref = doc.borrow_mut();
        if doc_ref.area_mode {
            let colour = doc_ref.area_colour;
            doc_ref.paint_area(unit.0, unit.1, colour);
            return;
        }
    }
    let tool = doc.borrow().tool;
    if tool == DrawerTool::Select {
        let mut doc_ref = doc.borrow_mut();
        let tolerance = 0.035 / view.zoom.max(0.25);
        let hit = hit_anchor(&doc_ref, local, bounds, view);
        if let Some((stroke, anchor, part)) = hit {
            doc_ref.selected_anchor = Some((stroke, anchor));
            interaction.borrow_mut().anchor_drag = Some(AnchorDrag {
                stroke,
                anchor,
                part,
                started: false,
            });
        } else {
            doc_ref.select_at(unit, tolerance);
            interaction.borrow_mut().anchor_drag = None;
        }
    } else if matches!(tool, DrawerTool::Curve | DrawerTool::Arc) {
        let mut inter = interaction.borrow_mut();
        let committed = match inter.draft.as_mut() {
            Some(draft) => draft.click(unit),
            None => {
                inter.draft = Draft::begin(tool, unit);
                None
            }
        };
        drop(inter);
        if let Some(stroke) = committed {
            interaction.borrow_mut().draft = None;
            doc.borrow_mut().commit_draft(Some(stroke));
        }
    } else if let Some(draft) = Draft::begin(tool, unit) {
        interaction.borrow_mut().draft = Some(draft);
    }
}

/// 指针移动。
pub(crate) fn moved(
    doc: &Rc<RefCell<DrawerDoc>>,
    interaction: &Rc<RefCell<DrawerInteraction>>,
    bounds: Rectangle,
    local: Point,
    shift: bool,
) {
    let mut inter = interaction.borrow_mut();
    if let Some(mut drag) = inter.anchor_drag {
        let view = doc.borrow().view;
        let unit = local_to_unit(local, bounds, view);
        let changed = {
            let mut doc_ref = doc.borrow_mut();
            doc_ref.drag_anchor(&mut drag, unit, shift)
        };
        inter.anchor_drag = Some(drag);
        if changed {
            return;
        }
    }
    if let Some(draft) = inter.draft.as_mut() {
        let view = doc.borrow().view;
        let unit = local_to_unit(local, bounds, view);
        draft.update_with_shift(unit, shift);
        return;
    }
    if let Some(start) = inter.pan_start {
        let mut doc_ref = doc.borrow_mut();
        let side = unit_side(bounds, doc_ref.view);
        doc_ref.view.pan.0 += (local.x - start.0) / side;
        doc_ref.view.pan.1 += (local.y - start.1) / side;
        inter.pan_start = Some((local.x, local.y));
    }
}

/// 中键按下（开始平移）。
pub(crate) fn middle_press(interaction: &Rc<RefCell<DrawerInteraction>>, local: Point) {
    interaction.borrow_mut().pan_start = Some((local.x, local.y));
}

/// 中键松开（结束平移）。
pub(crate) fn middle_release(interaction: &Rc<RefCell<DrawerInteraction>>) {
    interaction.borrow_mut().pan_start = None;
}

/// 左键松开（提交草稿）。
pub(crate) fn release(doc: &Rc<RefCell<DrawerDoc>>, interaction: &Rc<RefCell<DrawerInteraction>>) {
    let mut inter = interaction.borrow_mut();
    if inter.anchor_drag.take().is_some() {
        return;
    }
    if inter.draft.as_ref().is_some_and(Draft::is_multi_stage) {
        return;
    }
    if let Some(draft) = inter.draft.take() {
        let stroke = draft.to_stroke();
        drop(inter);
        doc.borrow_mut().commit_draft(stroke);
    }
}

/// 滚轮缩放（`lines` = 行模式滚轮）。
pub(crate) fn scroll(doc: &Rc<RefCell<DrawerDoc>>, lines: bool, up: bool) {
    let factor = if lines {
        if up { 1.1 } else { 1.0 / 1.1 }
    } else if up {
        1.05
    } else {
        1.0 / 1.05
    };
    doc.borrow_mut().view.zoom_by(factor);
}

/// 工具栏点击（按命中结果分发）。
fn toolbar_press(
    doc: &Rc<RefCell<DrawerDoc>>,
    interaction: &Rc<RefCell<DrawerInteraction>>,
    local: Point,
) {
    let Some(action) = toolbar_hit(local) else {
        return;
    };
    if matches!(action, ToolbarAction::Tool(_)) {
        interaction.borrow_mut().draft = None;
    }
    let mut doc_ref = doc.borrow_mut();
    match action {
        ToolbarAction::Tool(tool) => doc_ref.set_tool(tool),
        ToolbarAction::Undo => {
            doc_ref.undo();
        }
        ToolbarAction::Redo => {
            doc_ref.redo();
        }
        ToolbarAction::Delete => {
            doc_ref.delete_selected();
        }
    }
}

/// 工具栏按「条目下标」触发（egui 侧直接点名，无需模拟命中盒）。
pub(crate) fn toolbar_index(
    doc: &Rc<RefCell<DrawerDoc>>,
    interaction: &Rc<RefCell<DrawerInteraction>>,
    index: usize,
) {
    let tools = DrawerTool::ALL.len();
    if index < tools {
        interaction.borrow_mut().draft = None;
        doc.borrow_mut().set_tool(DrawerTool::ALL[index]);
        return;
    }
    let mut doc_ref = doc.borrow_mut();
    match index - tools {
        0 => {
            doc_ref.undo();
        }
        1 => {
            doc_ref.redo();
        }
        _ => {
            doc_ref.delete_selected();
        }
    }
}

/// 删除选中（Delete 键）。
pub(crate) fn delete(doc: &Rc<RefCell<DrawerDoc>>) {
    doc.borrow_mut().delete_selected();
}

/// 撤销 / 重做（Ctrl+Z / Ctrl+Y）。
pub(crate) fn undo(doc: &Rc<RefCell<DrawerDoc>>) {
    doc.borrow_mut().undo();
}

/// 重做。
pub(crate) fn redo(doc: &Rc<RefCell<DrawerDoc>>) {
    doc.borrow_mut().redo();
}

/// 失焦：清空瞬态（草稿 / 平移 / shift）。
pub(crate) fn unfocused(interaction: &Rc<RefCell<DrawerInteraction>>) {
    let mut inter = interaction.borrow_mut();
    inter.draft = None;
    inter.pan_start = None;
    inter.shift = false;
}
