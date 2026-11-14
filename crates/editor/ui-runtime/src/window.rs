//! 原生窗口 / surface 创建：从 `WindowManager::new` 收进运行时。
//!
//! Plan B 第三步：窗口与交换链的创建归运行时；应用层只拿到 `Arc<Window>`
//! 与 `lumino_gfx::Context`，不再直接拼 `WindowAttributes` / `create_surface`。

use std::sync::Arc;

use lumino_core::storage::ui_state::UiState;
use lumino_ui_core::constants::dimensions::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};
use winit::dpi;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// 已创建的窗口 + 交换链上下文。
pub struct NativeWindow {
    /// winit 窗口（运行时与 Host 共享）。
    pub window: Arc<Window>,
    /// gfx 上下文（device / queue / surface 配置）。
    pub gfx: lumino_gfx::Context,
    /// 建立交换链时使用的物理尺寸（0×0 窗口的兜底值也在这里）。
    pub size: winit::dpi::PhysicalSize<u32>,
}

impl NativeWindow {
    /// 按保存的窗口状态创建窗口与交换链。
    pub fn create(
        event_loop: &ActiveEventLoop,
        ui_state: &UiState,
        use_native_titlebar: bool,
    ) -> Result<Self, String> {
        let attributes = build_window_attributes(ui_state, use_native_titlebar);

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建窗口失败: {e}"))?,
        );

        // 先让窗口可见，再执行较重的 GPU 初始化，降低用户感知的启动延迟。
        window.set_visible(true);

        let physical_size = window.inner_size();
        // Android 首帧防护（2026-10-19 真机实测）：窗口刚创建时 `inner_size()` 可能
        // 为 0×0，wgpu-core 会以 `ZeroArea` 拒绝首次 configure（surface 保持未配置），
        // 随后首帧 `get_current_texture` 直接 fatal panic → Activity 被 finish。
        // 此时先按临时尺寸建立交换链，随后的 `Resized` 会把尺寸拉正到真实分辨率。
        let physical_size = if physical_size.width == 0 || physical_size.height == 0 {
            tracing::warn!("窗口尺寸为 0，先按 1280x720 建立临时交换链");
            winit::dpi::PhysicalSize::new(1280, 720)
        } else {
            physical_size
        };

        let gfx = lumino_gfx::Context::new_blocking(
            {
                let window = Arc::clone(&window);
                move |instance| instance.create_surface(Arc::clone(&window))
            },
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        Ok(Self {
            window,
            gfx,
            size: physical_size,
        })
    }
}

/// 构建窗口属性（尺寸 / 位置 / 最大化 / 平台标题栏）。
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
