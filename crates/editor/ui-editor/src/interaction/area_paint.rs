//! DRAW-032B：形状区域着色交互（pending 版）
//!
//! DEC-001：Bake 后形状对象只读，区域着色只对 **✓ 前的待确认对象** 开放。
//! 会话绑定 `pending_composed` 的单支结果：着色只改 pending model + 预览音符，
//! 不写文档、不 push 文档历史；✓ 才把预览音符落盘。
//!
//! 区域求解、切行与音符通道仍由 DRAW-032A 的 `AreaMap` 链路完成，本模块不重写算法。

use iced_core::Point;
use lumino_core::shape::{AreaSpot, ShapeFill, ShapeModel};
use lumino_editor_state::PendingShapeRef;
use lumino_message::Tool;

use crate::Editor;
use crate::interaction::line_tool::fill::spans::row_spans;
use crate::interaction::line_tool::geom;

/// 当前区域着色会话的绑定目标
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AreaPaintTarget {
    /// 待确认单支组合（`EditorState::pending_composed` 下标）
    PendingComposed { index: usize },
}

/// 当前区域着色会话
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AreaPaintState {
    /// 绑定目标
    pub target: AreaPaintTarget,
    /// 选中的绘制图形 id（用于检测选区是否变化）
    pub drawn_id: Option<u64>,
    /// 当前色号（0..=15；0 = 清除）
    pub colour: u8,
    /// 进入会话前的填充状态（透明色恢复用）
    pub original_fill: ShapeFill,
    /// 进入会话前的区域点（透明色恢复用）
    pub original_areas: Vec<AreaSpot>,
}

impl Editor {
    /// 进入区域着色模式：只接受单个 pending 形状/曲线（必要时先提升为单支组合）
    pub fn begin_area_paint(&mut self) -> bool {
        if self.current_tool() != Tool::ShapeSelect {
            return false;
        }
        // 先按来源判断闭合，避免「开放曲线」失败时留下一次无谓的提升（副作用）。
        let reference = {
            let mut selected = self.editor_state.shape_select.selected_shapes();
            let Some(shape) = selected.next() else {
                return false;
            };
            if selected.next().is_some() {
                return false;
            }
            shape.pending
        };
        match reference {
            Some(PendingShapeRef::Composed(index)) => {
                let Some(model) = self.area_paint_model(AreaPaintTarget::PendingComposed { index })
                else {
                    return false;
                };
                if !model.geometry.closed {
                    return false;
                }
            }
            Some(reference @ (PendingShapeRef::Shape(_) | PendingShapeRef::CurvePath(_))) => {
                let track = self.editor_state.data.current_track;
                let Some(source) = self.pending_source(reference) else {
                    return false;
                };
                let model = self.model_from_source_at_view(&source, track, 1.0);
                if !model.geometry.closed {
                    return false;
                }
            }
            _ => return false,
        }
        let Some((index, _promoted)) = self.pending_single_composed_for_ops() else {
            return false;
        };
        let target = AreaPaintTarget::PendingComposed { index };
        let Some(model) = self.area_paint_model(target) else {
            return false;
        };
        // 只有闭合区域才有“内部”可着色；开放曲线不能走形状再生成。
        if model.track == 0 || !model.geometry.closed {
            return false;
        }
        // 形状自身 colour_slot 默认 0；0 是“空”，作为画笔默认值会一上来就擦除。
        let colour = model
            .style
            .colour_slot
            .clamp(1, lumino_core::shape::most_colours(self.editor_state.use10));
        let original_fill = model.style.fill;
        let original_areas = model.style.areas.clone();
        let drawn_id = self
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .map(|shape| shape.id);
        self.area_paint = Some(AreaPaintState {
            target,
            drawn_id,
            colour,
            original_fill,
            original_areas,
        });
        self.shape_list_open = false;
        self.grid_cache.clear();
        true
    }

    /// 从 ShapeStore 列表进入区域着色
    ///
    /// DEC-001：列表里的模型都是已 Bake 的只读配方，一律拒绝。
    pub fn begin_area_paint_model(&mut self, _model_id: u64) -> bool {
        false
    }

    /// 会话绑定目标的模型
    fn area_paint_model(&self, target: AreaPaintTarget) -> Option<&ShapeModel> {
        match target {
            AreaPaintTarget::PendingComposed { index } => self
                .editor_state
                .pending_composed
                .get(index)?
                .results
                .first()
                .map(|part| &part.model),
        }
    }

    /// 区域着色会话对应的屏幕包围框
    pub(crate) fn area_paint_bounds(&self) -> Option<iced_core::Rectangle> {
        let target = self.area_paint.as_ref()?.target;
        let model = self.area_paint_model(target)?;
        let (tick_lo, key_lo, tick_hi, key_hi) = model.geometry.bounds()?;
        let a = self.line_pos_screen_pos((tick_lo, key_lo));
        let b = self.line_pos_screen_pos((tick_hi, key_hi));
        Some(iced_core::Rectangle::new(
            iced_core::Point::new(a.x.min(b.x) - 4.0, a.y.min(b.y) - 4.0),
            iced_core::Size::new((a.x - b.x).abs() + 8.0, (a.y - b.y).abs() + 8.0),
        ))
    }

    /// 打开 ShapeStore 模型列表（DRAW-041B）
    pub fn open_shape_list(&mut self) -> bool {
        let track = self.editor_state.data.current_track;
        if !self
            .editor_state
            .shape_store
            .iter()
            .any(|entry| entry.is_visible() && entry.model.track as usize == track)
        {
            return false;
        }
        self.area_paint = None;
        self.shape_list_open = true;
        self.grid_cache.clear();
        true
    }

    /// 关闭 ShapeStore 模型列表
    pub fn close_shape_list(&mut self) {
        if self.shape_list_open {
            self.shape_list_open = false;
            self.grid_cache.clear();
        }
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
        self.area_paint.as_ref().map(|state| state.colour)
    }

    /// 选择画笔色号（0..=15，启用通道 10 时 0..=16）
    pub fn set_area_paint_colour(&mut self, colour: u8) -> bool {
        let Some(state) = &mut self.area_paint else {
            return false;
        };
        let max = lumino_core::shape::most_colours(self.editor_state.use10);
        if colour > max {
            return false;
        }
        state.colour = colour;
        self.grid_cache.clear();
        true
    }

    /// 在逻辑坐标处着色；返回是否真的更新了 pending 预览
    ///
    /// `colour = 0` 是**透明色/撤销上色**：移除本次会话加的全部区域点，并恢复
    /// 进入区域着色前的填充状态（轮廓图形回到轮廓，填充图形回到基础填充），
    /// 不允许把整个形状直接清空。
    pub(crate) fn paint_area_at(&mut self, tick: f32, key: f32, colour: u8) -> bool {
        let Some(state) = self.area_paint.as_ref() else {
            return false;
        };
        let target = state.target;
        let drawn_id = state.drawn_id;
        let original_fill = state.original_fill;
        let original_areas = state.original_areas.clone();
        // 选区已变化：结束会话，避免把点画到旧形状上。
        if let Some(drawn_id) = drawn_id
            && !self
                .editor_state
                .shape_select
                .selected_shapes()
                .any(|shape| shape.id == drawn_id)
        {
            self.end_area_paint();
            return false;
        }
        let Some(mut model) = self.area_paint_model(target).cloned() else {
            self.end_area_paint();
            return false;
        };
        if !model.geometry.closed {
            return false;
        }
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

        let AreaPaintTarget::PendingComposed { index } = target;
        if colour == 0 {
            // 透明 = 撤销本次区域上色：恢复进入会话前的填充状态与区域点。
            if model.style.areas == original_areas && model.style.fill == original_fill {
                return false;
            }
            model.style.areas = original_areas;
            model.style.fill = original_fill;
            return self.commit_area_paint(index, model);
        }

        if model.style.areas.len() >= 256 {
            return false;
        }
        // 轮廓模型（Empty）第一次区域着色时显式切到 Fill。
        if model.style.fill == ShapeFill::Empty {
            model.style.fill = ShapeFill::Fill;
        }
        let u = ((tick - tick_lo) / width).clamp(0.0, 1.0);
        let v = ((key - key_lo) / height).clamp(0.0, 1.0);
        // 同一点重复点击只保留最后一次；不按正则化后的近似点去重，避免误删相邻区域。
        model
            .style
            .areas
            .retain(|spot| (spot.u - u).hypot(spot.v - v) > 0.02);
        model.style.areas.push(AreaSpot { u, v, colour });
        self.commit_area_paint(index, model)
    }

    /// 一次区域着色点击 = 一次 pending 撤销（不进入文档历史）
    fn commit_area_paint(&mut self, index: usize, model: ShapeModel) -> bool {
        let snapshot = self.editor_state.pending_composed.clone();
        self.update_pending_composed_model(index, model);
        self.pending_edit_undo.push(snapshot);
        while self.pending_edit_undo.len() > 64 {
            self.pending_edit_undo.remove(0);
        }
        self.pending_edit_redo.clear();
        self.pending_composed_redo.clear();
        true
    }

    /// 画布坐标处着色（供上下文菜单/网格事件复用）
    pub(crate) fn paint_area_at_screen(&mut self, pos: Point, colour: u8) -> bool {
        let (tick, key) = self.pos_to_line_raw(pos);
        self.paint_area_at(tick, key, colour)
    }
}

#[cfg(test)]
mod tests;
