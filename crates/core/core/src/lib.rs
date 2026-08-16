//! Lumino 核心基础类型
//!
//! 提供基础错误类型、共享领域类型、视图状态、滚动动画和存储配置。

pub mod error;
pub mod smooth_scroll;
pub mod storage;
pub mod types;
pub mod view_state;

pub use error::{CoreError, Result};
pub use smooth_scroll::SmoothScrollAnimation;
pub use types::{AudioAction, DotType, Language, NotePrecision, Tool};
pub use view_state::ViewState;
