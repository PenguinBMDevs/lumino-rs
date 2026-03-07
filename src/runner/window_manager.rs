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
}

impl WindowManager {
    /// 创建新的窗口管理器
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_state: &UiState,
        ui_config: &lumino_core::storage::config::UiConfig,
    ) -> Result<Self, String> {
        let attributes = Self::build_window_attributes(ui_state);

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建窗口失败: {e}"))?,
        );

        let physical_size = window.inner_size();

        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
            false,
        );

        window.set_visible(true);

        // 在 Windows 上设置自定义拉伸区域
        #[cfg(target_os = "windows")]
        crate::platform::windows::setup_resize_border(&window);

        Ok(Self {
            window,
            gfx,
            ui,
            modifiers: winit::keyboard::ModifiersState::default(),
            resized: false,
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

    /// 处理窗口事件
    pub fn handle_event(&mut self, event: WindowEvent, storage: &mut super::storage::Storage) {
        match event {
            WindowEvent::RedrawRequested => {
                self.handle_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_moved(position);
            }
            WindowEvent::Touch(touch) => {
                self.ui.cursor_moved(touch.location);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size, storage);
            }
            WindowEvent::Moved(pos) => {
                storage.ui_state.patch(|state| {
                    state.x = Some(pos.x);
                    state.y = Some(pos.y);
                });
            }
            WindowEvent::CloseRequested => {
                self.window.request_redraw();
            }
            _ => (),
        }

        self.ui.handle_events(event, self.modifiers);
    }

    /// 处理重绘
    fn handle_redraw(&mut self) {
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
        storage: &mut super::storage::Storage,
    ) {
        storage.ui_state.patch(|state| {
            state.w = size.width;
            state.h = size.height;
            state.is_maximized = self.window.is_maximized();
        });
        self.resized = true;
    }

    /// 处理窗口动作（最小化、最大化、关闭）
    pub fn handle_window_actions(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
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
                    event_loop.exit();
                }
            }
        }

        if self.ui.take_drag()
            && let Err(e) = self.window.drag_window()
        {
            tracing::warn!("拖动窗口失败: {}", e);
        }
    }

    /// 请求重绘
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 构建窗口属性
    fn build_window_attributes(ui_state: &UiState) -> WindowAttributes {
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
            attributes = attributes
                .with_decorations(false)
                .with_undecorated_shadow(true);
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes = attributes
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
        }

        attributes
    }
}
