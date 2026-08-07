//! 视频导出面板与导出覆盖层视图
//!
//! # 模块结构
//!
//! | 子模块 | 职责 | 行数 |
//! |--------|------|------|
//! | `helpers` | 进度详情/时长格式化/后端列表 | <span class="line-numbers">109</span> |
//! | `state` | 状态类型重导出 | <span class="line-numbers">6</span> |
//! | `layout` | pick_list 选择行、预览区域 | <span class="line-numbers">75</span> |
//! | `view` | 各区块渲染函数、公开入口 | <span class="line-numbers">228</span> |
//! | `handlers` | 覆盖层各状态视图 | <span class="line-numbers">180</span> |
//!
//! 主文件保留公开 API 入口，具体实现在子模块中。

pub mod counter_settings;
pub mod handlers;
pub mod helpers;
pub mod layout;
pub mod state;
pub mod view;

pub use view::{view_video_export_dialog, view_video_export_overlay};
