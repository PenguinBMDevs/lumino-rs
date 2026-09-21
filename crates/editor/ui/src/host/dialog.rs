//! Host 对话框和协作子模块 - 处理对话框状态和远程协作
//!
//! 视频导出相关方法见 `video` 子模块。
//!
//! 拆分说明：
//! - `self::state` — 对话框开关与状态 getter/setter
//! - `self::apply` — apply_* 数据回写与应用
//! - `self::collaboration` — 远端光标/选择/音符与协作状态同步

mod apply;
mod collaboration;
mod state;
mod video;
