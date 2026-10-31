//! 抽屉锚点/控制柄编辑（DRAW-033B1a）
//!
//! 落笔后选中锚点或控制柄拖动；与主曲线工具共用 `ShapeAnchor` 语义。

use lumino_core::shape::{AnchorBend, StrokeKind};

use super::DrawerDoc;

/// 拖动的是锚点本身还是控制柄
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorPart {
    /// 锚点位置
    Pos,
    /// 入控制柄
    In,
    /// 出控制柄
    Out,
}

/// 一次锚点拖动
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorDrag {
    /// stroke 下标
    pub stroke: usize,
    /// anchor 下标
    pub anchor: usize,
    /// 拖动部位
    pub part: AnchorPart,
    /// 是否已经 checkpoint（首次移动时才记录，避免空撤销）
    pub started: bool,
}

impl DrawerDoc {
    /// 开始一次可撤销编辑（锚点拖动首次移动时调用）
    pub fn begin_edit(&mut self) {
        self.checkpoint();
    }

    /// 标记文档已变更（不新增撤销点）
    pub fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// 重算某条 Curve stroke 的自动控制柄（1/3 邻段方向）
    pub fn recompute_auto_handles(&mut self, stroke_index: usize) {
        let Some(stroke) = self.strokes.get_mut(stroke_index) else {
            return;
        };
        if stroke.kind != StrokeKind::Curve {
            return;
        }
        let len = stroke.anchors.len();
        for index in 0..len {
            let bend = stroke.anchors[index].bend;
            let auto = stroke.anchors[index].handles_auto;
            if bend == AnchorBend::Corner {
                stroke.anchors[index].in_handle = (0.0, 0.0);
                stroke.anchors[index].out_handle = (0.0, 0.0);
                continue;
            }
            if !auto {
                continue;
            }
            let pos = stroke.anchors[index].pos;
            let mut in_handle = (0.0, 0.0);
            let mut out_handle = (0.0, 0.0);
            if index > 0 {
                let prev = stroke.anchors[index - 1].pos;
                in_handle = ((prev.0 - pos.0) / 3.0, (prev.1 - pos.1) / 3.0);
            }
            if index + 1 < len {
                let next = stroke.anchors[index + 1].pos;
                out_handle = ((next.0 - pos.0) / 3.0, (next.1 - pos.1) / 3.0);
            }
            stroke.anchors[index].in_handle = in_handle;
            stroke.anchors[index].out_handle = out_handle;
        }
    }

    /// 切换选中锚点的 Smooth/Corner（仅 Curve）
    pub fn toggle_selected_anchor_bend(&mut self) -> bool {
        let Some((stroke_index, anchor_index)) = self.selected_anchor else {
            return false;
        };
        let valid = self.strokes.get(stroke_index).is_some_and(|stroke| {
            stroke.kind == StrokeKind::Curve && anchor_index < stroke.anchors.len()
        });
        if !valid {
            return false;
        }
        self.checkpoint();
        {
            let stroke = &mut self.strokes[stroke_index];
            let anchor = &mut stroke.anchors[anchor_index];
            anchor.bend = match anchor.bend {
                AnchorBend::Smooth => AnchorBend::Corner,
                AnchorBend::Corner => AnchorBend::Smooth,
            };
            anchor.handles_auto = anchor.bend == AnchorBend::Smooth;
            if anchor.bend == AnchorBend::Corner {
                anchor.in_handle = (0.0, 0.0);
                anchor.out_handle = (0.0, 0.0);
            }
        }
        self.recompute_auto_handles(stroke_index);
        true
    }

    /// 应用一次锚点拖动；首次移动才 checkpoint。
    pub fn drag_anchor(&mut self, drag: &mut AnchorDrag, point: (f32, f32), shift: bool) -> bool {
        if !drag.started {
            self.checkpoint();
            drag.started = true;
        }
        let Some(stroke) = self.strokes.get_mut(drag.stroke) else {
            return false;
        };
        let Some(anchor) = stroke.anchors.get_mut(drag.anchor) else {
            return false;
        };
        match drag.part {
            AnchorPart::Pos => {
                anchor.pos = point;
            }
            AnchorPart::In => {
                let vector = (point.0 - anchor.pos.0, point.1 - anchor.pos.1);
                anchor.in_handle = snap_handle(vector, shift);
                anchor.handles_auto = false;
            }
            AnchorPart::Out => {
                let vector = (point.0 - anchor.pos.0, point.1 - anchor.pos.1);
                anchor.out_handle = snap_handle(vector, shift);
                anchor.handles_auto = false;
            }
        }
        if drag.part == AnchorPart::Pos && stroke.kind == StrokeKind::Curve {
            self.recompute_auto_handles(drag.stroke);
        }
        self.touch();
        true
    }
}

/// Shift 时把控制柄方向吸附到 15° 网格，长度不变
fn snap_handle(vector: (f32, f32), shift: bool) -> (f32, f32) {
    if !shift {
        return vector;
    }
    let length = (vector.0 * vector.0 + vector.1 * vector.1).sqrt();
    if length < 1e-6 {
        return vector;
    }
    let step = std::f32::consts::PI / 12.0;
    let angle = (vector.1.atan2(vector.0) / step).round() * step;
    (angle.cos() * length, angle.sin() * length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::shape::{ShapeAnchor, Stroke};

    fn curve_doc() -> DrawerDoc {
        let mut doc = DrawerDoc::default();
        let mut a = ShapeAnchor::new((0.2, 0.2));
        a.handles_auto = true;
        let mut b = ShapeAnchor::new((0.8, 0.2));
        b.handles_auto = true;
        doc.strokes.push(Stroke {
            kind: StrokeKind::Curve,
            anchors: vec![a, b],
            rect: None,
        });
        doc.selection.insert(0);
        doc
    }

    #[test]
    fn test_toggle_anchor_bend_zeroes_and_restores_handles() {
        let mut doc = curve_doc();
        doc.selected_anchor = Some((0, 1));
        assert!(doc.toggle_selected_anchor_bend());
        assert_eq!(doc.strokes[0].anchors[1].bend, AnchorBend::Corner);
        assert_eq!(doc.strokes[0].anchors[1].out_handle, (0.0, 0.0));
        assert!(doc.toggle_selected_anchor_bend());
        assert_eq!(doc.strokes[0].anchors[1].bend, AnchorBend::Smooth);
        assert!(doc.strokes[0].anchors[1].handles_auto);
    }

    #[test]
    fn test_drag_out_handle_shift_snaps_to_15_degrees() {
        let mut doc = curve_doc();
        let mut drag = AnchorDrag {
            stroke: 0,
            anchor: 0,
            part: AnchorPart::Out,
            started: false,
        };
        assert!(doc.drag_anchor(&mut drag, (0.2, 0.8), true));
        let handle = doc.strokes[0].anchors[0].out_handle;
        assert!((handle.0 - 0.0).abs() < 1e-5, "90° 吸附后横向应为 0");
        assert!((handle.1 - 0.6).abs() < 1e-5);
        assert!(!doc.strokes[0].anchors[0].handles_auto);
    }

    #[test]
    fn test_drag_pos_recomputes_auto_handles() {
        let mut doc = curve_doc();
        let mut drag = AnchorDrag {
            stroke: 0,
            anchor: 0,
            part: AnchorPart::Pos,
            started: false,
        };
        assert!(doc.drag_anchor(&mut drag, (0.3, 0.3), false));
        let anchor = &doc.strokes[0].anchors[0];
        assert_eq!(anchor.pos, (0.3, 0.3));
        assert!(anchor.handles_auto);
        assert!(anchor.out_handle.0 > 0.0, "自动柄应指向下一个锚点");
    }
}
