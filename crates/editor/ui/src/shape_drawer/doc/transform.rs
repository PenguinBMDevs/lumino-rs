//! 抽屉图形导入/导出与变换（DRAW-033B2/C）
//!
//! 从 doc.rs 拆出；坐标始终为单位框 0..=1。

use lumino_core::shape::{AreaSpot, Stroke, StrokeRole};

use super::DrawerDoc;

impl DrawerDoc {
    /// 粘贴 strokes（一次撤销单元；绝对坐标保持不变）
    pub fn paste_strokes(&mut self, strokes: &[Stroke]) -> bool {
        if strokes.is_empty() {
            return false;
        }
        self.checkpoint();
        for stroke in strokes {
            self.strokes.push(stroke.clone());
            self.roles.push(StrokeRole::default());
        }
        self.selection.clear();
        true
    }

    /// 水平翻转（一次撤销单元）
    pub fn flip_horizontal(&mut self) -> bool {
        self.transform_strokes(
            |anchor| {
                anchor.pos.0 = 1.0 - anchor.pos.0;
                anchor.in_handle.0 = -anchor.in_handle.0;
                anchor.out_handle.0 = -anchor.out_handle.0;
            },
            |area| area.u = 1.0 - area.u,
        )
    }

    /// 垂直翻转（一次撤销单元）
    pub fn flip_vertical(&mut self) -> bool {
        self.transform_strokes(
            |anchor| {
                anchor.pos.1 = 1.0 - anchor.pos.1;
                anchor.in_handle.1 = -anchor.in_handle.1;
                anchor.out_handle.1 = -anchor.out_handle.1;
            },
            |area| area.v = 1.0 - area.v,
        )
    }

    /// 顺时针旋转 90°（一次撤销单元）
    pub fn rotate90(&mut self) -> bool {
        self.transform_strokes(
            |anchor| {
                let (x, y) = anchor.pos;
                anchor.pos = (y, 1.0 - x);
                let (ix, iy) = anchor.in_handle;
                anchor.in_handle = (iy, -ix);
                let (ox, oy) = anchor.out_handle;
                anchor.out_handle = (oy, -ox);
            },
            |area| {
                let (u, v) = (area.u, area.v);
                area.u = v;
                area.v = 1.0 - u;
            },
        )
    }

    fn transform_strokes(
        &mut self,
        transform_anchor: impl Fn(&mut lumino_core::shape::ShapeAnchor),
        transform_area: impl Fn(&mut AreaSpot),
    ) -> bool {
        if self.strokes.is_empty() && self.areas.is_empty() {
            return false;
        }
        self.checkpoint();
        for stroke in &mut self.strokes {
            for anchor in &mut stroke.anchors {
                transform_anchor(anchor);
            }
            if let Some(rect) = &mut stroke.rect {
                let (lo_t, lo_k, hi_t, hi_k) =
                    (rect.tick_lo, rect.key_lo, rect.tick_hi, rect.key_hi);
                // 用两个角点经过同一变换后重新规范化，适用于翻转与 90° 旋转。
                let mut a = lumino_core::shape::ShapeAnchor::new((lo_t, lo_k));
                let mut b = lumino_core::shape::ShapeAnchor::new((hi_t, hi_k));
                transform_anchor(&mut a);
                transform_anchor(&mut b);
                *rect =
                    lumino_core::shape::ShapeRect::normalized(a.pos.0, a.pos.1, b.pos.0, b.pos.1);
            }
        }
        for area in &mut self.areas {
            transform_area(area);
        }
        true
    }
}

impl DrawerDoc {
    /// 缩放选中 strokes（无选中 = 全部）；围绕选区中心，结果夹在单位框内。
    pub fn scale_selection(&mut self, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        let indices: Vec<usize> = if self.selection.is_empty() {
            (0..self.strokes.len()).collect()
        } else {
            self.selection
                .iter()
                .copied()
                .filter(|&index| index < self.strokes.len())
                .collect()
        };
        if indices.is_empty() && self.areas.is_empty() {
            return false;
        }
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &index in &indices {
            for anchor in &self.strokes[index].anchors {
                min.0 = min.0.min(anchor.pos.0);
                min.1 = min.1.min(anchor.pos.1);
                max.0 = max.0.max(anchor.pos.0);
                max.1 = max.1.max(anchor.pos.1);
            }
        }
        if !min.0.is_finite() || !min.1.is_finite() {
            return false;
        }
        let pivot = ((min.0 + max.0) * 0.5, (min.1 + max.1) * 0.5);
        let mut factor = factor.clamp(0.25, 2.0);
        let half_x = (max.0 - min.0).abs() * 0.5;
        let half_y = (max.1 - min.1).abs() * 0.5;
        if half_x > 1e-6 {
            let avail = pivot.0.min(1.0 - pivot.0);
            factor = factor.min((avail / half_x).max(0.01));
        }
        if half_y > 1e-6 {
            let avail = pivot.1.min(1.0 - pivot.1);
            factor = factor.min((avail / half_y).max(0.01));
        }
        if (factor - 1.0).abs() < 1e-6 {
            return false;
        }
        self.checkpoint();
        for &index in &indices {
            for anchor in &mut self.strokes[index].anchors {
                anchor.pos = scale_point(anchor.pos, pivot, factor);
                anchor.in_handle = (anchor.in_handle.0 * factor, anchor.in_handle.1 * factor);
                anchor.out_handle = (anchor.out_handle.0 * factor, anchor.out_handle.1 * factor);
            }
            if let Some(rect) = &mut self.strokes[index].rect {
                let a = scale_point((rect.tick_lo, rect.key_lo), pivot, factor);
                let b = scale_point((rect.tick_hi, rect.key_hi), pivot, factor);
                *rect = lumino_core::shape::ShapeRect::normalized(a.0, a.1, b.0, b.1);
            }
        }
        for area in &mut self.areas {
            area.u = (pivot.0 + (area.u - pivot.0) * factor).clamp(0.0, 1.0);
            area.v = (pivot.1 + (area.v - pivot.1) * factor).clamp(0.0, 1.0);
        }
        true
    }
}

fn scale_point(point: (f32, f32), pivot: (f32, f32), factor: f32) -> (f32, f32) {
    (
        pivot.0 + (point.0 - pivot.0) * factor,
        pivot.1 + (point.1 - pivot.1) * factor,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::shape::{ShapeAnchor, StrokeKind};

    fn stroke(a: (f32, f32), b: (f32, f32)) -> Stroke {
        Stroke {
            kind: StrokeKind::Line,
            anchors: vec![ShapeAnchor::new(a), ShapeAnchor::new(b)],
            rect: None,
        }
    }

    #[test]
    fn test_scale_selection_halves_and_undoes() {
        let mut doc = DrawerDoc::default();
        doc.strokes.push(stroke((0.2, 0.2), (0.6, 0.6)));
        doc.selection.insert(0);
        assert!(doc.scale_selection(0.5));
        let anchors = &doc.strokes[0].anchors;
        assert!((anchors[0].pos.0 - 0.3).abs() < 1e-5);
        assert!((anchors[1].pos.0 - 0.5).abs() < 1e-5);
        assert!(doc.undo());
        let anchors = &doc.strokes[0].anchors;
        assert!((anchors[0].pos.0 - 0.2).abs() < 1e-5);
        assert!((anchors[1].pos.0 - 0.6).abs() < 1e-5);
    }

    #[test]
    fn test_scale_selection_clamps_to_unit_box() {
        let mut doc = DrawerDoc::default();
        doc.strokes.push(stroke((0.0, 0.0), (1.0, 1.0)));
        doc.selection.insert(0);
        assert!(!doc.scale_selection(2.0), "满框无法再放大");
    }
}
