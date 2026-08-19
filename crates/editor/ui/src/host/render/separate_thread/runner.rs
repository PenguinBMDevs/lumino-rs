//! 渲染数据收集与参数构建 — 收集各类 GPU 实例数据并构建渲染参数
//!
//! 子模块：
//! - `collect`: 渲染数据收集（collect_render_data, collect_arrangement_instances）
//! - `note_update`: 音符数据更新（update_note_data_for_wgpu_thread, build_preview_instances）
//! - `cc_bar`: CC 柱状条实例构建（build_cc_bar_instances）
//! - `params`: 渲染参数构建（build_render_params）

mod cc_bar;
mod collect;
mod note_update;
mod params;
