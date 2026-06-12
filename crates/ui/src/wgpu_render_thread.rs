//! WGPU 渲染线程模块 — 从 lumino-gfx 重导出
//!
//! 渲染线程已迁移到 `lumino-gfx` crate，此处为向后兼容的 shim。

pub use lumino_gfx::render_thread::{
    ControlCommand, RenderCommand, RenderParams, RenderStats, WgpuRenderThread,
};
