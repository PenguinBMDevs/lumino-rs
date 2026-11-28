//! 云浏览面板的几何常量（与 上一代 布局同口径）。

/// 面板内边距（生产 `container(...).padding(16)`）。
pub(super) const PAD: f32 = 16.0;
/// 行间距（生产 `column.spacing(8)`）。
pub(super) const GAP: f32 = 8.0;
/// 小按钮高度（12px 文本 + `padding([4,10])`）。
pub(super) const BTN_H: f32 = 26.0;
/// 行尾小按钮（12px + `padding([2,8])`）。
pub(super) const TINY_H: f32 = 22.0;
/// 列表行高（13px 名称 + `padding([3,8])`，实测 上一代 行距 27 逻辑）。
pub(super) const ROW_H: f32 = 23.0;
/// 输入框高度（`text_input` 默认内边距 + 13px 文本）。
pub(super) const INPUT_H: f32 = 31.5;
/// 大小列宽（生产 `text.width(Fixed(80))`）。
pub(super) const SIZE_W: f32 = 80.0;
/// 行尾按钮间距（生产 `row.spacing(6)`）。
pub(super) const BTN_GAP: f32 = 6.0;
