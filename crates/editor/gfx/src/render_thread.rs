//! WGPU 渲染线程模块
//!
//! 提供独立的 WGPU 渲染线程，用于在后台执行 GPU 渲染操作。
//!
//! 架构设计：
//! - 使用独立的渲染线程管理所有 GPU 资源
//! - 使用 mpsc 通道传递渲染命令
//! - 使用离屏纹理架构：渲染线程渲染到纹理，主线程复制到 Surface
//!
//! 子模块：
//! - `commands`: 渲染命令和控制命令定义
//! - `params`: 渲染参数结构体
//! - `stats`: 渲染统计信息
//! - `thread`: WgpuRenderThread 结构体和实现
//! - `render_loop`: 渲染循环实现

pub mod commands;
pub mod export_pipeline;
pub mod params;
pub mod render_loop;
pub mod stats;
pub mod thread;

pub use commands::{ControlCommand, FrameSender, RenderCommand};
pub use export_pipeline::ExportPipeline;
pub use params::RenderParams;
pub use stats::RenderStats;
pub use thread::WgpuRenderThread;
