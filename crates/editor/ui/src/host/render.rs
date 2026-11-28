//! Host 渲染子模块 - 处理 UI、网格和音符渲染
//!
//! 渲染架构（分离渲染线程模式）：
//! - 主窗口：UI 线程只负责状态更新和参数生成，WGPU 渲染在独立线程中（零拷贝数据共享）
//! - 轻量窗口（dialog/progress）：无 wgpu_render_thread，直接渲染 上一代 UI
//!
//! 子模块组织：
//! - `data`: 渲染数据类型定义
//! - `frame`: 帧准备逻辑（FPS计算、播放状态）
//! - `viewport`: 视口信息收集
//! - `onion_skin` / `cpu_onion`: GPU / CPU 洋葱皮（Q3 A/B）
//! - `separate_thread`: 分离渲染线程模式
//! - `ui`: 上一代 UI 渲染

use wgpu;

use crate::host::Host;

// 子模块声明
pub(crate) mod arrangement_geometry;
pub(crate) mod cpu_onion;
mod data;
pub(crate) mod egui_overlay;
mod frame;
pub(crate) mod mask_preview;
pub(crate) mod note_worker;
pub(crate) mod onion_skin;
mod separate_thread;
pub(crate) mod ui;
pub(crate) mod viewport;
mod waterfall_keyboard;

impl Host {
    /// 主渲染入口
    ///
    /// 根据是否启用 wgpu_render_thread 选择渲染路径：
    /// - 主窗口（已 `enable_separate_render_thread`）：走分离渲染线程，UI 线程只更新数据
    /// - 轻量窗口（dialog/progress）：直接渲染 上一代 UI（无音符/网格渲染器）
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        _gfx: &lumino_gfx::Context,
        egui_events: Vec<egui::Event>,
        egui_focused: bool,
    ) {
        // 通知 puffin 新的一帧开始 - 必须在 profile_function 之前调用
        puffin::GlobalProfiler::lock().new_frame();

        puffin::profile_function!();

        // 后台设备扫描结果 / 录制输入轮询必须在帧准备之前落盘：
        // 设备列表决定设置页模型，录制 NoteOn 决定本帧音符与播放头位置。
        self.poll_background_scans();
        self.poll_recording_input();
        self.maintain_settings_device_scan();

        // 帧准备：更新 FPS 和播放状态
        self.process_frame_preparation();

        // 更新光标位置（用于音符预览）
        {
            puffin::profile_scope!("update_cursor_for_preview");
            self.update_cursor_for_preview();
        }

        // 钢琴瀑布流面板键盘：在 GPU 上下文持有者处离屏渲染并缓存（按需）
        // 单独插桩：该函数未插桩，是播放期主线程同步 GPU 工作的可疑热点，
        // 用于区分卡顿是否来自瀑布流键盘离屏渲染（render_scene）本身。
        {
            puffin::profile_scope!("ensure_piano_waterfall_keyboard");
            self.ensure_piano_waterfall_keyboard();
        }

        if self.render_ctx.wgpu_render_thread.is_some() {
            // 主窗口：分离渲染线程模式
            self.render_with_separate_thread(frame, egui_events, egui_focused);
        } else {
            // 轻量窗口（dialog/progress）：直接渲染 上一代 UI
            if !self.skip_ui_rendering {
                self.render_ui(frame, view, None, egui_events, egui_focused);
            }
        }
    }

    /// 请求下一帧重绘（原「清除 上一代 UI 缓存」入口；egui 侧无缓存树）。
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 按需离屏渲染钢琴瀑布流面板键盘（真·裸 wgpu）
    ///
    /// 仅在面板可见且为钢琴瀑布流面板时渲染；其余情况清空缓存以释放纹理。
    /// 渲染产物为离屏纹理视图（`Arc<wgpu::TextureView>`），缓存于 `RightSidebar.piano_waterfall`，
    /// 由 上一代 `shader` 图元在自身渲染通道内直接合成（GPU→GPU，无 CPU 读回、不闪烁），
    /// 仅当（宽 / 高 / 键数 / 缩放 / 滚动 / 主音轨 / 音符数）任一参数变化时才重绘。
    ///
    /// 三条渲染路径的滚动来源**刻意不同**，改动时勿混用：
    /// - 全屏播放器（`AppMode::Waterfall`）：恒为固定下落，滚动由
    ///   `Editor::waterfall_player_scroll_x` 依播放位置计算，与卷帘自动滚动模式解耦；
    /// - 视频剪辑面板预览：以播放头处「素材源帧」为锚点（落点线 = 播放头）；
    /// - 右侧栏预览面板：与卷帘 X 缩放/滚动同步（它本就是卷帘的预览窗口）。
    ///
    /// 下落式音符直接复用渲染线程发布的活体 GPU 实例缓冲（只读 storage），
    /// 不重新上传音符数据——满足「禁止第二份拷贝」约束。
    pub(crate) fn ensure_piano_waterfall_keyboard(&mut self) {
        // 一次采集参数快照（含 take_note_data 与跨代次判废），后续三条路径按原顺序派发
        let params = self.collect_waterfall_params();
        if params.in_waterfall && self.render_fullscreen_waterfall(&params) {
            return;
        }
        if self.root.is_renderer_entry_active() {
            self.render_renderer_entry_preview(&params);
            return;
        }
        self.render_sidebar_preview(params);
    }
}
