//! Host 渲染子模块 - 处理 UI、网格和音符渲染
//!
//! 渲染架构（分离渲染线程模式）：
//! - 主窗口：UI 线程只负责状态更新和参数生成，WGPU 渲染在独立线程中（零拷贝数据共享）
//! - 轻量窗口（dialog/progress）：无 wgpu_render_thread，直接渲染 iced UI
//!
//! 子模块组织：
//! - `data`: 渲染数据类型定义
//! - `frame`: 帧准备逻辑（FPS计算、播放状态）
//! - `notes`: 音符实例更新
//! - `viewport`: 视口信息收集
//! - `separate_thread`: 分离渲染线程模式
//! - `ui`: iced UI 渲染

use iced_wgpu::wgpu;

use crate::host::Host;

// 子模块声明
mod data;
mod frame;
pub(crate) mod note_delta;
pub(crate) mod note_worker;
pub(crate) mod onion_skin;
mod separate_thread;
mod ui;
mod viewport;

// 公开子模块的公共类型

impl Host {
    /// 主渲染入口
    ///
    /// 根据是否启用 wgpu_render_thread 选择渲染路径：
    /// - 主窗口（已 `enable_separate_render_thread`）：走分离渲染线程，UI 线程只更新数据
    /// - 轻量窗口（dialog/progress）：直接渲染 iced UI（无音符/网格渲染器）
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 通知 puffin 新的一帧开始 - 必须在 profile_function 之前调用
        puffin::GlobalProfiler::lock().new_frame();

        puffin::profile_function!();

        // 帧准备：更新 FPS 和播放状态
        self.process_frame_preparation();

        // 更新光标位置（用于音符预览）
        self.update_cursor_for_preview();

        if self.render_ctx.wgpu_render_thread.is_some() {
            // 主窗口：分离渲染线程模式
            self.render_with_separate_thread(frame, gfx);
        } else {
            // 轻量窗口（dialog/progress）：直接渲染 iced UI
            if !self.skip_ui_rendering {
                self.render_iced_ui(frame, view);
            }
        }
    }

    /// 清除 UI 缓存以强制重绘
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.render_ctx.cache = std::mem::take(&mut self.render_ctx.cache);
    }
}
