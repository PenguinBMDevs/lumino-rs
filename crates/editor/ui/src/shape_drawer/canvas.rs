//! 形状抽屉画布的几何换算与事件逻辑（绘制后端无关）。
//!
//! 抽屉面板 / 分割线 / 画布由 egui 绘制（`ui-egui/src/shape_drawer.rs` +
//! `shape_canvas.rs`），事件逻辑与坐标换算集中在这里，供生产侧构建模型时复用。

pub(crate) mod draw;
pub(crate) mod input;
