//! 抽屉插入的待确认来源（DEBT-14 / DEC-001）
//!
//! 形状从抽屉进入工程必须经过待确认态：Insert 只产出 pending + 预览音符，
//! ✓ 才是唯一提交点。为了让「退回抽屉」能把用户送回原来的参数面，pending
//! 需要随身携带**原图 + 插入参数**——否则退回只能算重画。
//!
//! 这里只放纯数据（无 UI 依赖），使它既能挂在 `PendingComposed` 上，也能在
//! 测试里逐字段比对。

use super::drawing::ShapeDrawing;
use crate::shape::ShapeRect;

/// 抽屉插入参数（回填抽屉输入框 + 复核落位）
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeInsertSpec {
    /// 插入宽度（拍）
    pub beats: f32,
    /// 插入高度（key 数）
    pub keys: f32,
    /// 是否保持原图比例
    pub fit_contain: bool,
    /// 实际落位矩形（已含 `fit_contain` 的结果；tick/key 绝对坐标）
    pub rect: ShapeRect,
}

impl ShapeInsertSpec {
    /// 构造一份插入参数
    pub fn new(beats: f32, keys: f32, fit_contain: bool, rect: ShapeRect) -> Self {
        Self {
            beats,
            keys,
            fit_contain,
            rect,
        }
    }
}

/// 一次「抽屉 → 工程」的待确认来源
#[derive(Debug, Clone, PartialEq)]
pub struct DrawerInsert {
    /// 抽屉里的原图（退回时原样回填）
    pub drawing: ShapeDrawing,
    /// 插入参数（退回时回填输入框）
    pub spec: ShapeInsertSpec,
}

impl DrawerInsert {
    /// 构造一份抽屉插入来源
    pub fn new(drawing: ShapeDrawing, spec: ShapeInsertSpec) -> Self {
        Self { drawing, spec }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_spec_keeps_fields() {
        let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
        let spec = ShapeInsertSpec::new(4.0, 12.0, true, rect);
        assert!((spec.beats - 4.0).abs() < f32::EPSILON);
        assert!((spec.keys - 12.0).abs() < f32::EPSILON);
        assert!(spec.fit_contain);
        assert!((spec.rect.tick_lo - 960.0).abs() < f32::EPSILON);
    }
}
