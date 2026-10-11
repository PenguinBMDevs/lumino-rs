//! Lumino 核心基础类型
//!
//! 提供基础错误类型、共享领域类型、视图状态、滚动动画和存储配置。

/// 核心错误类型与结果别名
pub mod error;
/// 运行时文案目录（i18n 长尾覆盖层：原文即 key，见模块文档）
pub mod i18n;
/// 平滑滚动动画模块
pub mod smooth_scroll;
/// 应用存储配置与界面状态模块
pub mod storage;
/// 共享领域类型模块
pub mod types;
/// 视图状态模块
pub mod view_state;

pub use error::{CoreError, Result};
pub use smooth_scroll::SmoothScrollAnimation;
pub use types::{AudioAction, BrushConfig, DotType, Language, NotePrecision, Tool};
pub use view_state::ViewState;
