use std::sync::Arc;
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
}

impl WindowManager {
    /// 创建新的窗口管理器
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_state: &UiState,
        ui_config: &lumino_core::storage::config::UiConfig,
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
            window.clone(),
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        // 在后台预热对话框共享的 iced Engine，避免首个对话框创建时
        // 因重新编译 pipeline 阻塞事件循环 900ms+。
        lumino_ui::prewarm_dialog_shared_engine(&gfx);

        let mut ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
            false,
        );

        // 启用分离渲染线程：将 wgpu 渲染从 UI 线程分离到独立线程
        ui.enable_separate_render_thread();

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

    /// 处理窗口动作（最小化、最大化、关闭）
    pub fn handle_window_actions(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // 检查系统关闭请求
        if self.close_requested {
            self.quick_close();
            event_loop.exit();
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
                    self.quick_close();
                    event_loop.exit();
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
