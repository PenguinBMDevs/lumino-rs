//! 形状工具绘制状态（矩形 / 圆 / 三角形，拉出式拖拽绘制）
//!
//! 与曲线工具（`line_tool`）同构：拖拽拉出外接框 → 实时预览 → √ 批量确认生成音符。
//! 区别在于形状工具是「按住拖拽拉框」范式（曲线工具是「多点锚点」范式）。
//!
//! 画出的图形在确认（√）前作为临时叠加；确认后转成音符。与曲线工具一样，
//! 形状不保存为矢量对象，而是「固化为音符」，且**描边与填充分两条口径**：
//!
//! - **描边（轮廓，`filled = false`）**：走形状边界的**连续几何**
//!   （[`shape_outline_path`]）→ 与曲线工具轮廓**同源**的蜘蛛网式逐音高行解析
//!   （`ui-editor` 的 `line_tool::paths::path_notes`）：每个音高行一条音符、
//!   两两无缝连奏、长度自然变化，**不使用吸附精度**，切分档位也不作用于它；
//! - **填充（`filled = true`）**：仍是覆盖格点（[`shape_cells`]）——每格一条
//!   snap 长音符；开启「x 分音符」切分档位时按行合并连续格点后按全局网格切分。
//!
//! 填充桶（`fill_enabled`）决定确认时是否额外生成图形内部音符：
//! 既可在拉框时开着填充桶直接拉出实心图形，也可在拉出轮廓后再次用填充桶点选填充。
//!
//! **三角形朝向跟随拖拽方向**（[`ShapeSpec::apex_high`]）：顶点始终朝拖拽起点那一侧的
//! key 边——向下拉 = 屏幕正立、向上拉 = 倒立，拖拽过程中越过起点实时翻转。

mod geometry;
mod state;

pub use geometry::{
    CIRCLE_OUTLINE_SEGMENTS, effective_rect, point_in_shape, shape_cells, shape_outline_path,
    shape_vertices,
};

/// 形状类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeKind {
    /// 矩形（Shift → 正方形）
    #[default]
    Rectangle,
    /// 圆（Shift → 正圆，rx = ry）
    Circle,
    /// 三角形（Shift → 等边三角形）
    Triangle,
}

/// 形状工具交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShapeToolInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖拽拉框中（记录起点逻辑坐标 (tick, key)）
    Dragging {
        /// 拖拽起点逻辑坐标 (tick, key)
        start: (f32, f32),
    },
}

/// 形状几何参数：类型 + 外接框 + Shift 正图形约束 + 三角形顶点朝向
///
/// 渲染（预览 / 已确认图形高亮）、命中测试、格点枚举、描边折线四条腿都吃这一份参数，
/// 保证「看到的 = 判定的 = 生成的」。由 [`ShapeInstance::spec`] 或
/// `DrawnShapeSource::Shape` 的字段构造。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeSpec {
    /// 图形类型
    pub kind: ShapeKind,
    /// 外接框逻辑坐标 (tick_lo, key_lo, tick_hi, key_hi)（已规范化：lo <= hi）
    pub rect: (f32, f32, f32, f32),
    /// 绘制时是否按住 Shift（约束为正图形）
    pub shift_constrained: bool,
    /// 三角形**顶点**是否在 key 大的一侧（矩形 / 圆忽略本字段）
    ///
    /// 由拖拽方向决定：顶点始终朝**拖拽起点**那一侧的 key 边，故
    /// - 向下拉（key 递减 = 屏幕向下）⇒ `true` ⇒ 屏幕上**正立**（高度为正）；
    /// - 向上拉（key 递增 = 屏幕向上）⇒ `false` ⇒ **倒立**（高度为负）；
    /// - 拖拽过程中越过起点即实时翻转（见 `ShapeToolState::preview_rect`）。
    pub apex_high: bool,
}

/// 拖拽预览图形：`(几何参数, 是否填充)`。
pub type ShapePreview = (ShapeSpec, bool);

/// 单条待确认图形实例
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeInstance {
    /// 图形类型
    pub kind: ShapeKind,
    /// 外接框逻辑坐标 (tick_lo, key_lo, tick_hi, key_hi)（已规范化：lo <= hi）
    pub rect: (f32, f32, f32, f32),
    /// 绘制时是否按住 Shift（约束为正图形）
    pub shift_constrained: bool,
    /// 是否填充内部（颜料桶：绘制时开启或事后点击填充）
    pub filled: bool,
    /// 三角形顶点朝向（语义见 [`ShapeSpec::apex_high`]）
    pub apex_high: bool,
}

impl ShapeInstance {
    /// 取出四条几何腿共用的参数（`filled` 不属于几何，仍按需另传）
    pub fn spec(&self) -> ShapeSpec {
        ShapeSpec {
            kind: self.kind,
            rect: self.rect,
            shift_constrained: self.shift_constrained,
            apex_high: self.apex_high,
        }
    }
}

/// 形状工具状态
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeToolState {
    /// 当前选中的图形类型（由工具栏 `current_shape` 同步）
    pub shape_kind: ShapeKind,
    /// 拖拽交互状态
    pub interaction: ShapeToolInteraction,
    /// 拖拽当前点逻辑坐标（实时预览用）
    pub drag_current: (f32, f32),
    /// 绘制时颜料桶是否开启（用于新拉出图形的 `filled` 默认值）
    pub fill_enabled: bool,
    /// 待确认图形列表（√ 确认批量生成音符 / × 清空）
    pub shapes: Vec<ShapeInstance>,
}

#[cfg(test)]
mod tests;
