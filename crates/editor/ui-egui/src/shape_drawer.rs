//! 形状抽屉左侧面板（`DialogType::ShapeDrawer`）。
//!
//! 库列表 + 文件 / 编辑动作 + 面积填充 + 插入与缩放 + 插入按钮。
//! 右侧画布由画布层绘制（本层只覆盖面板区域，分割线由画布层拖动）。
//!
//! 几何对齐生产 `view/shape_drawer.rs`：面板内边距 10、列间距 6、标题 16px、
//! 库行按 `spacing(2)` 排列、名称输入与动作按钮组各占一行（放不下自动换行）。
//!
//! 结构：`action`（动作通道）、`layout`（常量与折行测量）、`widgets`（按钮 / 输入 /
//! 文本原语）、`library`（库列表）、`panel`（面板区块与插入）。对外仍从本模块根导出。

mod action;
mod layout;
mod library;
mod panel;
mod widgets;

pub use action::{ShapeDrawerAction, push_action, take_action};
pub use panel::show;
