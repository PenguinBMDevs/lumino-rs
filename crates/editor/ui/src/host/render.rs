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
use crate::right_sidebar::core::RESIZE_HANDLE_WIDTH;
use crate::right_sidebar::piano_waterfall::keyboard_renderer::{
    KEY_HEIGHT_RATIO, KeyboardRenderer, MAX_KEY_HEIGHT, MIN_KEY_HEIGHT,
};

// 子模块声明
mod data;
mod frame;
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

        // 钢琴瀑布流面板键盘：在 GPU 上下文持有者处离屏渲染并缓存（按需）
        self.ensure_piano_waterfall_keyboard();

        if self.render_ctx.wgpu_render_thread.is_some() {
            // 主窗口：分离渲染线程模式
            self.render_with_separate_thread(frame, gfx);
        } else {
            // 轻量窗口（dialog/progress）：直接渲染 iced UI
            if !self.skip_ui_rendering {
                self.render_iced_ui(frame, view, None);
            }
        }
    }

    /// 清除 UI 缓存以强制重绘
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.render_ctx.cache = std::mem::take(&mut self.render_ctx.cache);
    }

    /// 按需离屏渲染钢琴瀑布流面板键盘（真·裸 wgpu）
    ///
    /// 仅在面板可见且为钢琴瀑布流面板时渲染；其余情况清空缓存以释放纹理。
    /// 渲染产物为离屏纹理视图（`Arc<wgpu::TextureView>`），缓存于 `RightSidebar.piano_waterfall`，
    /// 由 iced `shader` 图元在自身渲染通道内直接合成（GPU→GPU，无 CPU 读回、不闪烁），
    /// 仅当（宽 / 高 / 键数 / 缩放 / 滚动 / 主音轨 / 音符数）任一参数变化时才重绘。
    ///
    /// 下落式音符直接复用渲染线程发布的活体 GPU 实例缓冲（只读 storage），
    /// 不重新上传音符数据——满足「禁止第二份拷贝」约束。
    pub(crate) fn ensure_piano_waterfall_keyboard(&mut self) {
        use crate::titlebar::mode_toggle::AppMode;

        let in_waterfall = self.root.state.current_mode == AppMode::Waterfall;

        // 键数跟随全局设置：开启 256 键扩展则为 256，否则 128
        let key_count: u32 = if self.root.settings.display.enable_256key {
            256
        } else {
            128
        };

        // 视口参数（与钢琴卷帘一致，驱动音符落点 / 时间流 / 主音轨蓝）
        let zoom_x = self.root.editor.editor_state.view.zoom_x;
        let scroll_x = self.root.editor.editor_state.view.scroll_x;
        let current_track = self.root.editor.editor_state.data.current_track as u32 + 1;

        // 复用渲染线程发布的活体 GPU 音符实例缓冲（零拷贝）
        let note_data = self
            .render_ctx
            .wgpu_render_thread
            .as_ref()
            .and_then(|t| t.take_note_data());
        let note_count = note_data.as_ref().map(|(_, c)| *c).unwrap_or(0);

        if in_waterfall {
            // ── 全屏瀑布流播放器：铺满主界面右侧内容区，复用同款离屏渲染 ──
            let size = *self.root.waterfall_player.size.borrow();
            let (width, height) = match size {
                Some(s) => s,
                // 尚无布局尺寸（首帧视图未构建），下一帧再渲染，避免 1x1 闪现
                None => return,
            };

            let mut sig: u64 = width as u64;
            sig = sig.wrapping_mul(31).wrapping_add(height as u64);
            sig = sig.wrapping_mul(31).wrapping_add(key_count as u64);
            sig = sig.wrapping_mul(31).wrapping_add(zoom_x as i64 as u64);
            sig = sig.wrapping_mul(31).wrapping_add(scroll_x as i64 as u64);
            sig = sig.wrapping_mul(31).wrapping_add(current_track as u64);
            sig = sig.wrapping_mul(31).wrapping_add(note_count as u64);

            let state = &mut self.root.waterfall_player;
            if state.cached_signature == Some(sig) {
                return; // 参数未变，复用已渲染纹理视图
            }

            let renderer = self
                .render_ctx
                .keyboard_renderer
                .get_or_insert_with(|| KeyboardRenderer::new(&self.render_ctx.device));

            if let Some(view) = renderer.render_scene(
                &self.render_ctx.device,
                &self.render_ctx.queue,
                width,
                height,
                key_count,
                note_data,
                zoom_x,
                scroll_x,
                current_track,
            ) {
                state.view = Some(view);
                state.cached_signature = Some(sig);
            }

            // 瀑布流模式下右侧栏预览被隐藏：释放其纹理与签名，停止一切渲染动作
            let rs = &mut self.root.right_sidebar.piano_waterfall;
            if rs.waterfall_view.is_some() {
                rs.waterfall_view = None;
                rs.cached_signature = None;
            }
            return;
        }

        // ── 右侧栏瀑布流预览 ──
        // 仅当右侧栏确实可见（且为瀑布流面板）时才渲染：关闭面板 / 切换面板 /
        // 进入瀑布流全屏模式 / 走带 / 导出面板 等任一情形均彻底停渲。
        let active = self.root.right_sidebar.panel_visible
            && self.root.right_sidebar.active_panel
                == crate::right_sidebar::RightSidebarPanel::PianoWaterfall
            && self.root.right_sidebar_visible();

        let state = &mut self.root.right_sidebar.piano_waterfall;
        if !active {
            // 面板不可见：释放已渲染纹理视图与签名，避免陈旧显示与显存占用
            if state.waterfall_view.is_some() {
                state.waterfall_view = None;
                state.cached_signature = None;
            }
            // 同步清空全屏播放器缓存（从瀑布流模式切回时）
            let fp = &mut self.root.waterfall_player;
            if fp.view.is_some() {
                fp.view = None;
                fp.cached_signature = None;
            }
            return;
        }

        // 键盘实际绘制宽度 = 面板内容宽（与显示宽度一致，无边框/留白，保证清晰不拉伸）
        let width = (self.root.right_sidebar.panel_width - RESIZE_HANDLE_WIDTH).max(1.0) as u32;
        // 面板内容高 = 窗口逻辑高（瀑布流占满面板纵轴，键盘贴底）
        let panel_height = self.render_ctx.viewport.logical_size().height.max(1.0);
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let height = (panel_height).max(kb_h + 1.0) as u32;

        // 脏判断签名：任意参数变化即重绘（滚动/缩放变化 → 瀑布流实时跟随）
        let mut sig: u64 = width as u64;
        sig = sig.wrapping_mul(31).wrapping_add(height as u64);
        sig = sig.wrapping_mul(31).wrapping_add(key_count as u64);
        sig = sig.wrapping_mul(31).wrapping_add(zoom_x as i64 as u64);
        sig = sig.wrapping_mul(31).wrapping_add(scroll_x as i64 as u64);
        sig = sig.wrapping_mul(31).wrapping_add(current_track as u64);
        sig = sig.wrapping_mul(31).wrapping_add(note_count as u64);

        if state.cached_signature == Some(sig) {
            return; // 参数未变，复用已渲染纹理视图
        }

        let renderer = self
            .render_ctx
            .keyboard_renderer
            .get_or_insert_with(|| KeyboardRenderer::new(&self.render_ctx.device));

        // 同步离屏渲染：返回 Some 即拿到本帧纹理视图（无 CPU 读回、无异步）；
        // iced `shader` 图元会在自身渲染通道内直接采样该视图合成（GPU→GPU，不闪烁）。
        if let Some(view) = renderer.render_scene(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            width,
            height,
            key_count,
            note_data,
            zoom_x,
            scroll_x,
            current_track,
        ) {
            state.waterfall_view = Some(view);
            state.cached_signature = Some(sig);
        }
    }
}
