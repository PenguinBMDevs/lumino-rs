//! 云文件浏览面板（`DialogType::CloudBrowser`）与断连提醒（`CloudNotice`）。
//!
//! 浏览面板：整窗 base 底 + 内边距 16，自上而下工具栏（设备下拉 + 刷新 / 断开 /
//! 粘贴 / 清空剪贴板）、导航行（返回 + 当前路径）、新建文件夹行、可选的保存目标区
//! （`background.weak` 底 + 圆角 6 + 内边距 10）、可滚动条目列表（行距 4）、
//! 12px 操作提示。条目行 = 名称（可点进目录）+ 80 宽大小 + 行尾小按钮。
//!
//! 动作只回传意图（下标 / 路径 / 字段文本），磁盘操作仍由 Runner 完成。
//!
//! 结构：`action`（动作与单帧通道）、`layout`（几何常量）、`widgets`（按钮 / 输入 /
//! 文本原语）、`panel`（浏览面板与断连提醒）、`list`（条目列表与行渲染）。对外仍从
//! 本模块根导出，调用方路径不变。

mod action;
mod layout;
mod list;
mod panel;
mod widgets;

pub use action::{CloudBrowserAction, push_action, take_action};
pub use panel::{show, show_notice};
