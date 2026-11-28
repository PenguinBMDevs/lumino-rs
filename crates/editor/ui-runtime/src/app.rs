//! 最小原生运行时：winit 事件循环 + wgpu 27 Surface + egui 0.37 + 自有 bridge。
//!
//! 结构：`state`（窗口 / GPU / 画布 / 帧间缓存）、`events`（winit → egui 输入与
//! 编辑器动作）、`render`（音符上传 / 渲染参数 / 画布纹理 / 外壳 / present）。
//! 对外仍导出 [`NativeApp`] 与 [`run`]。

mod events;
mod render;
mod state;

#[cfg(test)]
mod tests;

pub use state::NativeApp;

/// 启动原生 egui 运行时探针。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    crate::runtime::run(|_event_loop| NativeApp::default())?;
    Ok(())
}
