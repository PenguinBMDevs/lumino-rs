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
        use lumino_ui_core::sidebar_event::GroupId;

        // 视频剪辑面板（渲染器首级）：不应渲染钢琴卷帘的任何内容（网格/音符/标尺），
        // 仅保留瀑布流离屏预览（由 ensure_piano_waterfall_keyboard 的 is_renderer_entry 分支处理）
        // 与 iced UI，避免其他面板内容透出或 GPU 浪费。
        let is_renderer_clip = self.root.sidebar.active_group == Some(GroupId::Renderer)
            && !self.root.sidebar.audio_export_visible
            && !self.root.sidebar.video_export_visible
            && self.root.state.current_mode != AppMode::Waterfall;
        if is_renderer_clip {
            if !self.skip_ui_rendering {
                let view = frame
                    .texture
                    .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
                let bg = self.root.theme().palette().background;
                self.render_iced_ui(frame, &view, Some(bg));
            }
            return;
        }

        // 始终驱动渲染线程：保证音符实例缓冲持续发布，供瀑布流播放器读取实时落键。
        self.redraw_separate_thread();

        // 全屏瀑布流播放器：与卷帘 3D 场景隔离，清屏后仅叠加 iced UI。
        // 纵向卷帘已接入 wgpu 转置管线（复用 MIDI GPU 数据，瀑布流风格纵向），
        // 与横向同走离屏拷贝 + iced 覆盖层，不再早期返回，避免 wgpu 网格被清空。
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

        // 视频剪辑面板时跳过钢琴卷帘的离屏渲染（网格/音符/标尺），避免与剪辑预览叠加
        {
            use crate::titlebar::mode_toggle::AppMode;
            use lumino_ui_core::sidebar_event::GroupId;
            let is_renderer_clip = self.root.sidebar.active_group == Some(GroupId::Renderer)
                && !self.root.sidebar.audio_export_visible
                && !self.root.sidebar.video_export_visible
                && self.root.state.current_mode != AppMode::Waterfall;
            if is_renderer_clip {
                return;
            }
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
