//! 形状工具状态机：拖拽交互、实时预览与待确认图形生成

use super::geometry::normalize_rect;
use super::{ShapeInstance, ShapeKind, ShapePreview, ShapeToolInteraction, ShapeToolState};

impl ShapeToolState {
    /// 重置整个状态（含已拉出图形与当前图形类型）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 仅清除临时绘制状态（拖拽 / 待确认图形 / 填充桶开关），保留当前图形类型
    ///
    /// 用于切换工具时清理残留，但保留用户在工具栏选中的图形类型（矩形/圆/三角），
    /// 避免切换工具再切回后被重置为默认矩形。
    pub fn clear_pending(&mut self) {
        self.interaction = ShapeToolInteraction::None;
        self.drag_current = (0.0, 0.0);
        self.fill_enabled = false;
        self.shapes.clear();
    }

    /// 设置当前图形类型
    pub fn set_shape_kind(&mut self, kind: ShapeKind) {
        self.shape_kind = kind;
    }

    /// 设置填充桶开关
    pub fn set_fill_enabled(&mut self, enabled: bool) {
        self.fill_enabled = enabled;
    }

    /// 是否有待确认图形
    pub fn has_pending(&self) -> bool {
        !self.shapes.is_empty()
    }

    /// 是否正在拖拽
    pub fn is_dragging(&self) -> bool {
        matches!(self.interaction, ShapeToolInteraction::Dragging { .. })
    }

    /// 开始拖拽（记录起点）
    pub fn begin_drag(&mut self, start: (f32, f32)) {
        self.interaction = ShapeToolInteraction::Dragging { start };
        self.drag_current = start;
    }

    /// 更新拖拽当前点
    pub fn update_drag(&mut self, current: (f32, f32)) {
        self.drag_current = current;
    }

    /// 结束拖拽：由起点 + 当前点计算外接框，生成待确认图形
    ///
    /// - `snap`：吸附精度（tick），用于判定拖拽过短无效；
    /// - `shift_constrained`：绘制时是否按住 Shift（约束正图形）。
    ///
    /// 返回 `None` 表示拖拽过短 / 无效，已丢弃。
    pub fn end_drag(&mut self, snap: f32, shift_constrained: bool) -> Option<ShapeInstance> {
        let start = match self.interaction {
            ShapeToolInteraction::Dragging { start } => start,
            ShapeToolInteraction::None => return None,
        };
        self.interaction = ShapeToolInteraction::None;
        let current = self.drag_current;
        // 拖拽过短（小于半格）视为无效，丢弃（避免误触）
        if (current.0 - start.0).abs() < snap * 0.5 && (current.1 - start.1).abs() < 0.5 {
            return None;
        }
        let rect = normalize_rect(start, current);
        let instance = ShapeInstance {
            kind: self.shape_kind,
            rect,
            shift_constrained,
            filled: self.fill_enabled,
        };
        self.shapes.push(instance.clone());
        Some(instance)
    }

    /// 当前正在拖拽的预览图形（若有）：返回 (类型, 外接框, Shift约束, 填充)
    pub fn preview_rect(&self, shift_constrained: bool) -> Option<ShapePreview> {
        if let ShapeToolInteraction::Dragging { start } = self.interaction {
            let rect = normalize_rect(start, self.drag_current);
            Some((self.shape_kind, rect, shift_constrained, self.fill_enabled))
        } else {
            None
        }
    }
}
