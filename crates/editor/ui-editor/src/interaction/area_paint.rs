//! DRAW-032B：形状区域着色交互（最小）
//!
//! 交互只做三件事：
//! 1. 从当前唯一选中的已确认图形绑定 `ShapeModel`，进入着色模式；
//! 2. 画布左键 → 在形状 box 内追加一个 `AreaSpot`，调用 `edit_shape` 再生成；
//! 3. 右键 / 0 色 → 写 `colour = 0`，让该区域静音。
//!
//! 区域求解、切行与音符通道仍由 DRAW-032A 的 `AreaMap` 链路完成，本模块不重写算法。

use iced_core::Point;
use lumino_core::shape::AreaSpot;
use lumino_message::Tool;

use crate::Editor;
use crate::interaction::line_tool::fill::spans::row_spans;
use crate::interaction::line_tool::geom;

/// 当前区域着色会话
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaPaintState {
    /// 选中的绘制图形 id（用于检测选区是否变化）
    pub drawn_id: u64,
    /// ShapeStore 模型 id
    pub model_id: u64,
    /// 当前色号（0..=15；0 = 清除）
    pub colour: u8,
}

impl Editor {
    /// 进入区域着色模式；仅鼠标工具下唯一选中、且能绑定 ShapeModel 的图形可用
    pub fn begin_area_paint(&mut self) -> bool {
        if self.current_tool() != Tool::ShapeSelect
            || self.editor_state.shape_select.selection_len() != 1
        {
            return false;
        }
        let Some(drawn) = self
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .cloned()
        else {
            return false;
        };
        let Some(model_id) = self.ensure_model_for_drawn(drawn.id) else {
            return false;
        };
        let Some(model) = self.editor_state.shape_store.model(model_id) else {
            return false;
        };
        // 形状自身 colour_slot 默认 0；0 是“空”，作为画笔默认值会一上来就擦除。
        let colour = model.style.colour_slot.clamp(1, 15);
        self.area_paint = Some(AreaPaintState {
            drawn_id: drawn.id,
            model_id,
            colour,
        });
        self.grid_cache.clear();
        true
    }

    /// 退出区域着色模式
    pub fn end_area_paint(&mut self) {
        if self.area_paint.take().is_some() {
            self.grid_cache.clear();
        }
    }

    /// 是否处于区域着色模式
    pub fn area_paint_active(&self) -> bool {
        self.area_paint.is_some()
    }

    /// 当前画笔色号（无会话时返回 `None`）
    pub fn area_paint_colour(&self) -> Option<u8> {
        self.area_paint.map(|state| state.colour)
    }

    /// 选择画笔色号（0..=15）
    pub fn set_area_paint_colour(&mut self, colour: u8) -> bool {
        let Some(state) = &mut self.area_paint else {
            return false;
        };
        if colour > 15 {
            return false;
        }
        state.colour = colour;
        self.grid_cache.clear();
        true
    }

    /// 在逻辑坐标处着色；返回是否真的提交了一次形状编辑
    pub(crate) fn paint_area_at(&mut self, tick: f32, key: f32, colour: u8) -> bool {
        let Some(state) = self.area_paint else {
            return false;
        };
        // 选区已变化：结束会话，避免把点画到旧形状上。
        if !self
            .editor_state
            .shape_select
            .selected_shapes()
            .any(|shape| shape.id == state.drawn_id)
        {
            self.end_area_paint();
            return false;
        }
        let Some(mut model) = self.editor_state.shape_store.model(state.model_id).cloned() else {
            self.end_area_paint();
            return false;
        };
        let Some((tick_lo, key_lo, tick_hi, key_hi)) = model.geometry.bounds() else {
            return false;
        };
        let width = tick_hi - tick_lo;
        let height = key_hi - key_lo;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || tick < tick_lo
            || tick > tick_hi
            || key < key_lo
            || key > key_hi
        {
            return false;
        }
        if model.style.areas.len() >= 256 {
            return false;
        }
        // 闭合点列形状再做一次“点在轮廓内”检查，避免在 bbox 角落留下无效 spot。
        if model.geometry.closed && model.geometry.anchors.len() >= 3 {
            let mut poly: Vec<(f32, f32)> = geom::flatten_path(&model.geometry.anchors)
                .into_iter()
                .map(|(t, k)| (t as f32, k as f32))
                .collect();
            if poly.first() != poly.last()
                && let Some(first) = poly.first().copied()
            {
                poly.push(first);
            }
            let inside = row_spans(&poly, key)
                .iter()
                .any(|(start, end)| tick >= *start && tick <= *end);
            if !inside {
                return false;
            }
        }
        let u = ((tick - tick_lo) / width).clamp(0.0, 1.0);
        let v = ((key - key_lo) / height).clamp(0.0, 1.0);
        // 同一点重复点击只保留最后一次；不按正则化后的近似点去重，避免误删相邻区域。
        model
            .style
            .areas
            .retain(|spot| (spot.u - u).hypot(spot.v - v) > 0.02);
        model.style.areas.push(AreaSpot { u, v, colour });
        if self.edit_shape(state.model_id, model) {
            self.grid_cache.clear();
            true
        } else {
            false
        }
    }

    /// 画布坐标处着色（供上下文菜单/网格事件复用）
    pub(crate) fn paint_area_at_screen(&mut self, pos: Point, colour: u8) -> bool {
        let (tick, key) = self.pos_to_line_raw(pos);
        self.paint_area_at(tick, key, colour)
    }
}

#[cfg(test)]
mod tests;
