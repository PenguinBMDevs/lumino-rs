//! 核心入口点 — 分离渲染线程模式的主入口、验证和参数发送
//!
//! 提供分离渲染线程模式的主入口、验证和参数发送方法。

use crate::host::Host;

impl Host {
    /// 分离渲染线程模式的主渲染入口
    pub(crate) fn render_with_separate_thread(
        &mut self,
        frame: &iced_wgpu::wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        use crate::titlebar::mode_toggle::AppMode;

        // 始终驱动渲染线程：保证音符实例缓冲持续发布，供瀑布流播放器读取实时落键。
        self.redraw_separate_thread();

        // 全屏瀑布流播放器模式：与钢琴卷帘完全隔离。
        // 不再把卷帘 3D 场景 blit 到 surface；改为清屏为应用背景后仅叠加 iced UI
        // （瀑布流 + 键盘），卷帘的网格/音符不会透出到播放器背景。
        if self.root.state.current_mode == AppMode::Waterfall {
            if !self.skip_ui_rendering {
                let view = frame
                    .texture
                    .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
                let bg = self.root.theme().palette().background;
                self.render_iced_ui(frame, &view, Some(bg));
            }
            return;
        }

        // 将离屏渲染结果拷贝到 Surface
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.copy_offscreen_to_surface(frame, &gfx.device, &gfx.queue);
        }

        // iced UI 覆盖层渲染到同一 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
            self.render_iced_ui(frame, &view, None);
        }
    }

    /// 分离渲染线程模式的主渲染逻辑
    ///
    /// UI 线程只负责：
    /// 1. 更新状态
    /// 2. 生成渲染参数
    /// 3. 写入音符数据到双缓冲
    /// 4. 发送渲染参数到 WGPU 线程
    pub(super) fn redraw_separate_thread(&mut self) {
        puffin::profile_function!();
        puffin::profile_scope!("redraw_separate_thread");

        if !self.validate_render_thread_ready() {
            return;
        }

        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);

        // 发送渲染参数到 WGPU 线程（非阻塞）
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.send_params(params);
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.render_ctx.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        true
    }
}
