//! 形状工具状态机：拖拽交互、实时预览与待确认图形生成

use super::geometry::normalize_rect;
use super::{
    ShapeInstance, ShapeKind, ShapePreview, ShapeSpec, ShapeToolInteraction, ShapeToolState,
};

/// 三角形顶点朝向：顶点朝**拖拽起点**那一侧的 key 边
///
/// - 向下拉（`current_key <= start_key`，屏幕向下）⇒ `true` ⇒ 屏幕上**正立**；
/// - 向上拉（`current_key > start_key`，屏幕向上）⇒ `false` ⇒ **倒立**；
/// - 拖拽过程中越过起点 ⇒ 结果翻转 ⇒ 预览实时变向（本函数在每帧预览与松手时各算一次）。
///
/// 语义与判定只在**逻辑 key 轴**上（key 大 = 音高高 = 横向卷帘的屏幕上方、纵向卷帘的
/// 屏幕右方），故纵向卷帘转置后依然自洽。
fn apex_high_of(start: (f32, f32), current: (f32, f32)) -> bool {
    current.1 <= start.1
}

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

    /// 仅收敛**未完成的拉框交互**，保留全部待确认图形与当前图形类型
    ///
    /// 供切换工具时调用：待确认图形是用户的绘制产物，不因切换工具被丢弃
    /// （见 `EditorState::set_tool`）；清空只发生在显式 × / √。
    pub fn cancel_interaction(&mut self) {
        self.interaction = ShapeToolInteraction::None;
        self.drag_current = (0.0, 0.0);
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
            apex_high: apex_high_of(start, current),
        };
        self.shapes.push(instance.clone());
        Some(instance)
    }

    /// 当前正在拖拽的预览图形（若有）：返回 (几何参数, 是否填充)
    ///
    /// 每帧由起点 + 当前点重算：三角形的顶点朝向随拖拽方向**实时**改变
    /// （越过起点即翻转，见 `apex_high_of`）。
    pub fn preview_rect(&self, shift_constrained: bool) -> Option<ShapePreview> {
        if let ShapeToolInteraction::Dragging { start } = self.interaction {
            let current = self.drag_current;
            Some((
                ShapeSpec {
                    kind: self.shape_kind,
                    rect: normalize_rect(start, current),
                    shift_constrained,
                    apex_high: apex_high_of(start, current),
                },
                self.fill_enabled,
            ))
        } else {
            None
        }
    }
}
