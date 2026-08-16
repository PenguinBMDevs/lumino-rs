//! 自定义精度对话框动作

use crate::DotType;
use crate::types::TupletType;

/// 自定义精度对话框动作
#[derive(Debug, Clone)]
pub enum CustomPrecisionAction {
    /// 打开对话框
    OpenDialog,
    /// 关闭对话框
    CloseDialog,
    /// 确认自定义精度
    Confirm,
    /// 三连音数量变更
    TupletCountChanged(String),
    /// 三连音类型变更
    TupletTypeChanged(TupletType),
    /// 符点类型变更
    DotTypeChanged(DotType),
    /// 分音符值变更
    NoteValueChanged(String),
    /// 除数变更
    DivisorChanged(String),
}
