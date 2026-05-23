//! 钢琴卷帘网格绘制模块
//!
//! 该模块已拆分为以下子模块：
//! - `state`: Canvas状态管理
//! - `theme`: 主题颜色工具
//! - `utils`: 工具函数（is_key_dark, parse_color）
//! - `keyboard`: 钢琴键盘绘制
//! - `keys`: 琴键分隔线绘制
//! - `ruler`: 时间轴标尺绘制
//! - `bars`: 小节线/网格线绘制
//! - `remote_cursors`: 远程光标渲染
//! - `selection_box`: 选择框渲染
//! - `playback_indicator`: 播放指示线渲染
//! - `program`: PianoRollGrid 结构体定义
//! - `program_impl`: Program trait 实现（事件处理、绘制）
//! - `editor_impl`: Editor 网格线实例更新方法

pub mod bars;
pub mod editor_impl;
pub mod keyboard;
pub mod keys;
pub mod loop_range;
pub mod playback_indicator;
pub mod program;
pub mod program_impl;
pub mod remote_cursors;
pub mod ruler;
pub mod selection_box;
pub mod state;
pub mod theme;
pub mod utils;

pub use loop_range::{LoopHitTest, LoopRange};
pub use program::PianoRollGrid;
pub use state::CanvasState;
