//! Root 视图子模块。
//!
//! iced 视图层已删除（egui 是唯一绘制通道），这里只剩：
//! - `egui_chrome`：生产状态 → egui 模型的唯一构造入口；
//! - `status`：状态栏用到的工程统计（音符总数）。

mod egui_chrome;
mod status;
mod visibility;
