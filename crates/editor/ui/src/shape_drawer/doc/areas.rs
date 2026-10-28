//! 抽屉区域着色求解（DRAW-033C）
//!
//! 从 doc.rs 拆出；只允许单条闭合 stroke，复用 AreaMap / paint_area。

use std::rc::Rc;

use lumino_core::shape::{AreaMap, AreaSpot};

use super::DrawerDoc;

impl DrawerDoc {
    /// 是否可进入区域着色：只允许一条闭合 stroke。
    pub fn can_area_paint(&self) -> bool {
        self.area_paths().is_some()
    }

    /// 设置区域模式；多笔画/开放图形拒绝进入。
    pub fn set_area_mode(&mut self, enabled: bool) -> bool {
        if enabled && !self.can_area_paint() {
            self.area_mode = false;
            return false;
        }
        self.area_mode = enabled;
        true
    }

    /// 设置区域色号（0..=15）
    pub fn set_area_colour(&mut self, colour: u8) -> bool {
        if colour > 15 {
            return false;
        }
        self.area_colour = colour;
        true
    }

    /// 构建当前区域网格（未缓存版本）
    fn build_area_map(&self) -> Option<AreaMap> {
        let paths = self.area_paths()?;
        let mut map = AreaMap::rasterize(&paths, false);
        for spot in &self.areas {
            if spot.colour <= 15 {
                map.paint_area(spot.u, spot.v, spot.colour);
            }
        }
        Some(map)
    }

    /// 取区域网格缓存；revision 变化时自动重建。
    pub fn area_map_cached(&self) -> Option<Rc<AreaMap>> {
        if !self.can_area_paint() {
            return None;
        }
        if let Some((revision, map)) = self.area_cache.borrow().as_ref()
            && *revision == self.revision
        {
            return Some(map.clone());
        }
        let map = Rc::new(self.build_area_map()?);
        *self.area_cache.borrow_mut() = Some((self.revision, map.clone()));
        Some(map)
    }

    /// 在点击位置着色/清除；一次点击 = 一次撤销单元。
    pub fn paint_area(&mut self, u: f32, v: f32, colour: u8) -> bool {
        if colour > 15 || !self.can_area_paint() {
            return false;
        }
        let Some(base) = self.area_map_cached() else {
            return false;
        };
        let mut map = (*base).clone();
        if !map.paint_area(u, v, colour) {
            return false;
        }
        if self.areas.len() >= 1024 {
            return false;
        }
        self.checkpoint();
        self.areas.push(AreaSpot { u, v, colour });
        true
    }

    /// 清空全部区域点
    pub fn clear_areas(&mut self) -> bool {
        if self.areas.is_empty() {
            return false;
        }
        self.checkpoint();
        self.areas.clear();
        true
    }

    /// 单闭合路径；多笔画/开放路径返回 None
    fn area_paths(&self) -> Option<Vec<Vec<(f32, f32)>>> {
        if self.strokes.len() != 1 {
            return None;
        }
        let stroke = &self.strokes[0];
        let mut points: Vec<(f32, f32)> = stroke.anchors.iter().map(|anchor| anchor.pos).collect();
        if points.len() < 3 {
            let rect = stroke.rect?;
            points = vec![
                (rect.tick_lo, rect.key_lo),
                (rect.tick_hi, rect.key_lo),
                (rect.tick_hi, rect.key_hi),
                (rect.tick_lo, rect.key_hi),
            ];
        }
        if points.len() < 3 {
            return None;
        }
        let closed = stroke.rect.is_some()
            || points
                .first()
                .zip(points.last())
                .is_some_and(|(first, last)| {
                    (first.0 - last.0).abs() <= 1e-4 && (first.1 - last.1).abs() <= 1e-4
                });
        if !closed {
            return None;
        }
        if points.first() != points.last() {
            points.push(points[0]);
        }
        Some(vec![points])
    }
}
