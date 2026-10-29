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
