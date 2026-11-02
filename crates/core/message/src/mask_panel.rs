//! 遮罩参数面板动作（DRAW-010）
//!
//! 面板只承载「用户意图」，参数装配在 `lumino-ui` 的 `Root`：
//! pending 形状即时预览，已 Bake 走草稿 + Apply（一次撤销单元）。

use lumino_core::shape::MaskShow;

/// 遮罩参数面板动作
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaskPanelAction {
    /// 关闭面板
    Close,
    /// 开关遮罩（false = 移除标记）
    SetEnabled(bool),
    /// 带宽（keys，0..=32）
    SetSize(f32),
    /// 取反：图案只显示在遮罩之外（挖洞）
    SetInvert(bool),
    /// 自身音符发声（false = 纯切割，只留虚线边框）
    SetOwn(bool),
    /// 区域来源：内部 / 沿线
    SetShow(MaskShow),
    /// 已 Bake：把草稿写入（一次 `edit_shape` 撤销单元）
    Apply,
}
