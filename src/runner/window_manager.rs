use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lumino_core::storage::ui_state::UiState;

mod event;
pub(crate) mod geometry;

/// 主窗口管理器
///
/// 负责管理主窗口的生命周期、渲染和事件处理
pub struct WindowManager {
    /// 窗口实例
    window: Arc<winit::window::Window>,
    /// 图形上下文
    gfx: lumino_gfx::Context,
    /// UI 主机
    ui: lumino_ui::Host,
    /// egui 输入缓冲（主窗口持有；Host 只负责翻译）
    egui_input: lumino_ui::host::OverlayInput,
    /// 修饰键状态
    modifiers: winit::keyboard::ModifiersState,
    /// 是否需要调整大小
    resized: bool,
    /// 是否需要渲染一帧（点击优先：事件队列排空后统一补帧）
    redraw_pending: bool,
    /// 是否请求关闭窗口
    close_requested: bool,
    /// 本地保存进行中标志（与 RunnerInner 共享：保存期间禁止关闭）
    saving: Arc<AtomicBool>,
    /// 云端上传进行中标志（与 RunnerInner 共享）
    cloud_saving: Arc<AtomicBool>,
    /// 待关闭标志：保存期间用户请求关闭 → 保存完成后自动退出
    pub(crate) close_pending: bool,
    /// 窗口关闭被保存确认对话框挂起（工程存在未保存更改）
    ///
    /// `handle_window_actions` 检测到关闭请求且工程有未保存更改时置位，
    /// 由 Runner 在 `about_to_wait` 中读取并弹出保存确认对话框，置位后清空。
    pub(crate) deferred_save_confirm_close: bool,
    /// REND-019：重连后窗口几何微扰的待恢复目标尺寸
    /// （见 [`Self::nudge_window_geometry`]；`Some` = 微扰进行中）。
    pending_size_restore: Option<winit::dpi::PhysicalSize<u32>>,
    /// REND-019：微扰前窗口是否为最大化（微扰会退出最大化，完成后自动恢复）。
    pending_restore_maximized: bool,
    /// REND-019 周边（R4 P2 反馈）：微扰恢复最大化后需要把存活对话框重新置前
    /// （"取消最大化→重最大化"会把主窗抬到端窗之上；由 Runner 每帧读取执行一次）。
    pub(crate) raise_dialogs_pending: bool,
    /// Android 首帧前是否已用「当前窗口尺寸」重配过一次交换链。
    ///
    /// `WindowManager::new` 建链时窗口表面可能尚未 finalize，configure 会被 hal
    /// 拒绝并让 wgpu-core 丢掉 presentation（此后取帧恒 NotConfigured）。首帧前
    /// 用真实 `inner_size()` 再配一次即可安装 presentation。
    android_reconfigured_once: bool,
}

impl WindowManager {
    /// 创建新的窗口管理器
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_state: &UiState,
        ui_config: &lumino_core::storage::config::UiConfig,
        saving: Arc<AtomicBool>,
        cloud_saving: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let native = lumino_ui_runtime::NativeWindow::create(
            event_loop,
            ui_state,
            ui_config.use_native_titlebar,
        )?;
        let window = native.window;
        let gfx = native.gfx;
        let physical_size = native.size;

        // 在后台预热系统字体缓存，使首次打开设置对话框时字体下拉菜单
        // 的列表已就绪，不阻塞 UI 线程。字体扫描在后台线程调用
        // get_cached_fonts() 完成，OnceLock 保证只扫一次。
        lumino_note_core::font_scanner::prewarm_font_cache();

        let mut ui = lumino_ui::Host::new(
            lumino_ui::host::host_window(window.clone()),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
            false,
        );

        // 启用分离渲染线程：将 wgpu 渲染从 UI 线程分离到独立线程
        ui.enable_separate_render_thread();

        // 启动即初始化空白工程（默认 2 轨：Conductor + Setup）。
        // 2026-08 根因修复：EditorData::new 时 document=None + current_track=0，
        // 启动后直接画音符会被 current_track==0 拦截（Conductor 禁止放置）。
        // 此处与菜单新建/关闭文件共用 init_blank_project，逻辑一致且幂等。
        ui.init_blank_project();

        // 在 Windows 上设置自定义拉伸区域（仅自定义标题栏模式）
        #[cfg(target_os = "windows")]
        if !ui_config.use_native_titlebar {
            crate::platform::windows::setup_resize_border(&window)?;
        }

        Ok(Self {
            window,
            gfx,
            ui,
            egui_input: lumino_ui::host::OverlayInput::default(),
            modifiers: winit::keyboard::ModifiersState::default(),
            resized: false,
            redraw_pending: false,
            close_requested: false,
            saving,
            cloud_saving,
            close_pending: false,
            deferred_save_confirm_close: false,
            pending_size_restore: None,
            pending_restore_maximized: false,
            raise_dialogs_pending: false,
            android_reconfigured_once: false,
        })
    }

    /// 获取窗口引用
    pub fn window(&self) -> &Arc<winit::window::Window> {
        &self.window
    }

    /// 获取 UI 主机的可变引用
    pub fn ui_mut(&mut self) -> &mut lumino_ui::Host {
        &mut self.ui
    }

    /// 获取 UI 主机的引用
    pub fn ui(&self) -> &lumino_ui::Host {
        &self.ui
    }

    /// REND-019 PR-C：重连——窗口 Context 与 Host 重绑到新 device（幂等）。
    #[allow(dead_code)]
    pub(crate) fn rebind_gpu(&mut self, epoch: lumino_gfx::GpuEpoch, gpu: &lumino_gfx::SharedGpu) {
        self.gfx.rebind_device(epoch, gpu);
        self.ui.rebind_gpu(epoch, gpu);
    }

    /// REND-019（重连后"输入+画面脱钩"修复）：窗口几何微扰。
    ///
    /// 实测（2026-10-17，用户端 + 自动复现）：device 重建重连完成后，窗口进入
    /// 「交换链 / DWM 组合关联失效」状态——状态机、日志、winit 事件交付全部
    /// 正常，但**输入不再产生任何可见/状态效果、画面停在旧帧**；对窗口做
    /// **任意一次真实尺寸变化**（取消最大化 / 拖拽到边缘）即整体恢复。
    /// 对策：重连完成时把窗口宽度缩 4px，并在下一次 `Resized` 到达后恢复
    /// 原尺寸——等效用户那次"尺寸变化解锁"，全程无可见副作用。
    ///
    /// 注：最大化窗口中 `request_inner_size` 会先退出最大化（Windows 行为）；
    /// 记录原最大化状态，解锁后自动恢复最大化（`handle_resize` 的恢复步）。
    pub(crate) fn nudge_window_geometry(&mut self) {
        let size = self.window.inner_size();
        if size.width < 64 {
            return;
        }
        self.pending_size_restore = Some(size);
        self.pending_restore_maximized = self.window.is_maximized();
        let nudged = winit::dpi::PhysicalSize::new(size.width - 4, size.height);
        let _ = self.window.request_inner_size(nudged);
    }

    /// 请求重绘窗口
    /// REND-019 周边：取走"需要置前存活对话框"标志（微扰恢复最大化后置位；
    /// 见字段注释）。返回 true 时由 Runner 调用 DialogManager 置前各端窗。
    pub(crate) fn take_raise_dialogs_pending(&mut self) -> bool {
        std::mem::take(&mut self.raise_dialogs_pending)
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 快速关闭：先销毁窗口（隐藏），再退出事件循环
    ///
    /// 用户点击关闭按钮时，窗口立即隐藏，让用户感知到"已关闭"，
    /// 剩余进程清理在 `about_to_wait` 当前迭代结束后自然完成。
    fn quick_close(&mut self) {
        // 先隐藏窗口，让用户立即感知到关闭
        self.window.set_visible(false);
        // 重置关闭请求标记，防止重复触发
        self.close_requested = false;
    }

    /// 本地保存/云端上传是否进行中（与 RunnerInner 共享的标志）
    fn is_saving(&self) -> bool {
        self.saving.load(Ordering::SeqCst) || self.cloud_saving.load(Ordering::SeqCst)
    }

    /// 处理窗口动作（最小化、最大化、关闭）
    pub fn handle_window_actions(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // 检查系统关闭请求
        if self.close_requested {
            self.handle_close_request(event_loop);
            return;
        }

        if let Some(action) = self.ui.take_window_action() {
            use lumino_ui::window::TrafficAction;
            match action {
                TrafficAction::Minimize => {
                    self.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    let is_maximized = self.window.is_maximized();
                    self.window.set_maximized(!is_maximized);
                }
                TrafficAction::Close => {
                    self.handle_close_request(event_loop);
                }
            }
        }

        if self.ui.take_drag() {
            // 使用 winit 的 drag_window 处理窗口拖动
            if let Err(e) = self.window.drag_window() {
                tracing::warn!("拖动窗口失败: {}", e);
                self.ui.release_left_mouse_button();
            }
        }
    }

    /// 统一的窗口关闭处理（系统关闭请求 / 自制标题栏关闭按钮共用）
    ///
    /// 处理顺序：
    /// 1. 保存/云端上传进行中 → 转为 `close_pending`，保存完成后自动退出；
    /// 2. 工程存在未保存更改 → 置位 `deferred_save_confirm_close`，
    ///    交由 Runner 弹出「是否保留未保存的更改」确认对话框，**暂不关闭**；
    /// 3. 否则立即快速关闭并退出事件循环。
    fn handle_close_request(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // 保存期间禁止关闭软件：关闭请求转为待关闭，保存完成后自动退出
        if self.is_saving() {
            tracing::info!("保存进行中，关闭请求延迟到保存完成");
            self.close_pending = true;
            self.close_requested = false;
            return;
        }

        // 工程存在未保存更改：交给 Runner 弹出保存确认对话框，暂不关闭窗口
        if self.ui.is_project_modified() {
            tracing::debug!("窗口关闭：工程存在未保存更改，挂起保存确认对话框");
            self.deferred_save_confirm_close = true;
            self.close_requested = false;
            return;
        }

        self.quick_close();
        event_loop.exit();
    }

    /// 取走挂起的保存确认关闭请求（Runner 在 `about_to_wait` 中调用）
    ///
    /// 读取并清空 `deferred_save_confirm_close` 标志，True 表示本次窗口关闭
    /// 因工程未保存更改而挂起，需弹出保存确认对话框。
    pub(crate) fn take_deferred_save_confirm_close(&mut self) -> bool {
        let pending = self.deferred_save_confirm_close;
        self.deferred_save_confirm_close = false;
        pending
    }
}
