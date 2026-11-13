//! 钢琴卷帘的共享几何与输入语义模块。
//!
//! iced 绘制层已删除（网格 / 键盘 / 标尺 / 选择框全部由 wgpu 与 egui 外壳绘制），
//! 这里只保留输入分派、命中测试与生产侧共用的几何。

pub mod area_paint_box;
pub mod brush_tool_box;
pub mod canvas_input;
pub mod confirm_buttons;
pub mod cursor;
pub mod i2m_box;
pub mod line_tool_box;
pub mod loop_range;
pub mod program;
pub mod shape_list_box;
pub mod shape_tool_box;
pub mod state;
pub mod text_tool_box;
pub mod theme;
pub mod utils;
pub mod vertical_mouse;

pub use loop_range::{LoopHitTest, LoopRange};
pub use program::PianoRollGrid;
pub use state::GridInteractionState;
