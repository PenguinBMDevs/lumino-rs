use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::{dpi, event::WindowEvent, window::WindowAttributes};

use lumino_core::storage::ui_state::UiState;
use lumino_ui::constants::dimensions::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};

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
    /// 修饰键状态
    modifiers: winit::keyboard::ModifiersState,
    /// 是否需要调整大小
    resized: bool,
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
        let attributes = Self::build_window_attributes(ui_state, ui_config.use_native_titlebar);

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建窗口失败: {e}"))?,
        );

        // 先让窗口可见，再执行较重的 GPU/Host 初始化，降低用户感知的启动延迟。
        window.set_visible(true);

        let physical_size = window.inner_size();

        let gfx = lumino_gfx::Context::new_blocking(
            Arc::clone(&window),
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        // yinhe 模式：同步注入 Material Icons 到全局字体系统，需早于首个 Host/Engine 创建
        // 与 `UiConfig.program_font_name` 共存，正文缺省族 vs 图标显式族 `Material Symbols Rounded` 高优不可覆盖
        #[cfg(feature = "yinhe")]
        lumino_ui_yinhe::material_icons::ensure_loaded();

        // 在后台预热对话框共享的 iced Engine，避免首个对话框创建时
        // 因重新编译 pipeline 阻塞事件循环 900ms+。
        lumino_ui::prewarm_dialog_shared_engine(&gfx);

        // 在后台预热系统字体缓存，使首次打开设置对话框时字体下拉菜单
        // 的列表已就绪，不阻塞 UI 线程。字体扫描在后台线程调用
        // get_cached_fonts() 完成，OnceLock 保证只扫一次。
        lumino_note_core::font_scanner::prewarm_font_cache();

        let mut ui = lumino_ui::Host::new(
            Arc::clone(&window),
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
            modifiers: winit::keyboard::ModifiersState::default(),
            resized: false,
            close_requested: false,
            saving,
            cloud_saving,
            close_pending: false,
            deferred_save_confirm_close: false,
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

    /// 请求重绘窗口
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 处理窗口事件
    pub fn handle_event(&mut self, event: WindowEvent, storage: &mut crate::storage::Storage) {
        match event {
            WindowEvent::RedrawRequested => {
                self.handle_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_moved(position);
            }
            WindowEvent::Touch(touch) => {
                // 触控事件只更新 UI 光标位置，不处理窗口拖动
                // 窗口拖动由鼠标事件处理
                self.ui.cursor_moved(touch.location);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size, storage);
            }
            WindowEvent::Moved(pos) => {
                // 更新存储的位置信息
                storage.ui_state.patch(|state| {
                    state.x = Some(pos.x);
                    state.y = Some(pos.y);
                });
            }
            WindowEvent::CloseRequested => {
                self.close_requested = true;
            }
            _ => (),
        }

        self.ui.handle_events(event, self.modifiers);
    }

    /// 处理重绘
    fn handle_redraw(&mut self) {
        puffin::profile_function!();

        if self.resized {
            let size = self.window.inner_size();
            self.ui.resize(size.width, size.height);
            self.gfx.resize(size.width, size.height);
            self.resized = false;
        }

        if self
            .gfx
            .with_frame(|frame, view| self.ui.redraw_requested(frame, view, &self.gfx))
            .is_err()
        {
            self.window.request_redraw();
        }
    }

    /// 处理窗口大小改变
    fn handle_resize(
        &mut self,
        size: winit::dpi::PhysicalSize<u32>,
        storage: &mut crate::storage::Storage,
    ) {
        storage.ui_state.patch(|state| {
            state.w = size.width;
            state.h = size.height;
            state.is_maximized = self.window.is_maximized();
        });
        self.resized = true;
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

    /// 构建窗口属性
    fn build_window_attributes(
        ui_state: &UiState,
        #[allow(unused_variables)] use_native_titlebar: bool,
    ) -> WindowAttributes {
        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(dpi::LogicalSize {
                width: MIN_WINDOW_WIDTH,
                height: MIN_WINDOW_HEIGHT,
            })
            .with_inner_size(dpi::LogicalSize {
                width: ui_state.w,
                height: ui_state.h,
            })
            .with_maximized(ui_state.is_maximized)
            .with_title("Lumino")
            .with_visible(false);

        if let (Some(x), Some(y)) = (ui_state.x, ui_state.y)
            && !ui_state.is_maximized
        {
            attributes = attributes.with_position(dpi::LogicalPosition { x, y });
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            if use_native_titlebar {
                // 使用系统标题栏
                attributes = attributes.with_decorations(true);
            } else {
                // 使用自定义标题栏
                attributes = attributes
                    .with_decorations(false)
                    .with_undecorated_shadow(true);
            }
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            if use_native_titlebar {
                // 使用系统标题栏
                attributes = attributes.with_titlebar_transparent(false);
            } else {
                // 使用自定义标题栏
                attributes = attributes
                    .with_titlebar_transparent(true)
                    .with_fullsize_content_view(true);
            }
        }

        attributes
    }
}
