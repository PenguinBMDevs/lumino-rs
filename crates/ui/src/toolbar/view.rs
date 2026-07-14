//! Toolbar 视图渲染子模块
//!
//! 已按职责拆分为以下子模块：
//! - toolbar_view: 主视图函数（入口）
//! - controls:    控件渲染函数（播放控制、循环、工具选择、调整手柄、撤销/重做）
//! - status:      状态显示函数（精度选择、自动滚动、协作按钮）

mod controls;
mod status;
mod toolbar_view;
