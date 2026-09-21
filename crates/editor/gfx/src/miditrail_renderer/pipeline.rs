//! Miditrail 3D 渲染管线创建
//!
//! Normal 与 Top 视图共用同一实例缓冲布局（见 `miditrail_top.wgsl` 头注释），
//! 区别仅在于着色器模块（3D 光照 vs flat）与深度写入策略（两者一致：
//! 音符不写深度、琴键写深度，琴键最后绘制覆盖音符）。

mod aura;
mod buffers;
mod driven;
mod render;

pub use aura::create_aura_render_pipeline;
pub use buffers::{create_bind_group_layout, create_buffers, create_quad_index_buffer};
pub use driven::{create_driven_group_layout, create_note_driven_pipeline};
pub use render::{
    create_note_render_pipeline, create_render_pipeline, create_top_note_render_pipeline,
    create_top_render_pipeline,
};
