//! 设备检查警告窗：通用浮窗 + 用户操作回传。
//!
//! 生命周期由 [`super::overlay_window::OverlayWindow`] 统一实现；本模块只保留
//! 专用 Host 构造、关闭语义（置标志而非立即销毁）与用户选择回传。
//! 警告窗只在「首启/未抑制 + 检测失败」时短暂存在；用户选择由
//! [`lumino_ui::Host::take_device_warning_choice`] 取出，交由 Runner 决策。

use std::sync::Arc;

use winit::{dpi, event::WindowEvent, event_loop::ActiveEventLoop, window::WindowId};

use lumino_core::storage::config::UiConfig;
use lumino_core::tr;

use super::overlay_window::{ClosePolicy, OverlaySpec, OverlayWindow};

/// 警告窗宽度（逻辑像素）
pub(crate) const DEVICE_WARNING_WINDOW_WIDTH: u32 = 560;
/// 警告窗高度（逻辑像素）
pub(crate) const DEVICE_WARNING_WINDOW_HEIGHT: u32 = 200;

/// 设备检查警告窗管理器
pub(crate) struct DeviceWarningWindow {
    overlay: OverlayWindow,
}

impl DeviceWarningWindow {
    /// 创建警告窗。任何一步失败都返回 `Err`，由调用方走原生提示框兜底。
    pub(crate) fn create(
        event_loop: &ActiveEventLoop,
        ui_config: &UiConfig,
        detail: &str,
    ) -> Result<Self, String> {
        let spec = OverlaySpec {
            size: dpi::LogicalSize {
                width: DEVICE_WARNING_WINDOW_WIDTH,
                height: DEVICE_WARNING_WINDOW_HEIGHT,
            },
            title: tr!("GPU 兼容性检查").to_string(),
            resizable: false,
            buttons: None,
            // 关闭视为「确认并关闭」：置标志交 `take_action`，不立即销毁窗口
            close_policy: ClosePolicy::Flag,
            // 警告窗内容含悬停/按下态，输入后需立即重绘
            redraw_on_input: true,
            initial_redraw: true,
        };
        let overlay = OverlayWindow::create_with_host(
            event_loop,
            ui_config,
            &spec,
            None,
            |window, gfx, size| {
                lumino_ui::Host::new_device_warning(
                    lumino_ui::host::host_window(Arc::clone(window)),
                    size.width,
                    size.height,
                    ui_config,
                    gfx,
                    detail,
                )
            },
        )
        .map_err(|e| format!("创建警告窗口失败: {e}"))?;
        Ok(Self { overlay })
    }

    /// 是否为该警告窗的窗口 ID
    pub(crate) fn is_window(&self, window_id: WindowId) -> bool {
        self.overlay.is_window(window_id)
    }

    /// REND-019 PR-C：重连——警告窗存活时重绑到新 device（不存在则 no-op）。
    #[allow(dead_code)]
    pub(crate) fn rebind_gpu(&mut self, epoch: lumino_gfx::GpuEpoch, gpu: &lumino_gfx::SharedGpu) {
        self.overlay.rebind_gpu(epoch, gpu);
    }

    /// 处理窗口事件（重绘 / 光标 / 尺寸 / 关闭）
    pub(crate) fn handle_event(&mut self, event: WindowEvent) {
        self.overlay.handle_event(event);
    }

    /// 取走用户操作（UI 按钮优先；其次窗口关闭视为「确认并关闭」）
    pub(crate) fn take_action(&mut self) -> Option<lumino_ui::window::DeviceWarningAction> {
        if let Some(action) = self
            .overlay
            .ui_mut()
            .and_then(|ui| ui.take_device_warning_choice())
        {
            return Some(action);
        }
        if self.overlay.take_close_requested() {
            return Some(lumino_ui::window::DeviceWarningAction::Quit);
        }
        None
    }

    /// 关闭并释放窗口资源
    pub(crate) fn close(&mut self) {
        self.overlay.close();
    }
}
