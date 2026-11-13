//! 标题栏数据（菜单 / 模式切换）。
//!
//! 主窗口与对话框窗口的标题栏（菜单标签 / logo / 拖动 / 双击最大化 / 红绿灯）
//! 全部由 egui 绘制并接管（`chrome::titlebar` + `chrome::popup_titlebar`），
//! 旧版 视图层删除后这里只保留菜单数据与模式状态。

pub mod menu;
pub mod mode_toggle;
