//! MIDI 加载进度窗口管理器（薄壳）
//!
//! 窗口生命周期由 [`super::overlay_window::OverlayWindow`] 统一实现；
//! 本模块只保留进度数据源、主窗进度镜像与对外 API。

use tokio::sync::mpsc;
use winit::{
    dpi, event::WindowEvent, event_loop::ActiveEventLoop, window::Window, window::WindowButtons,
    window::WindowId,
};

use lumino_core::storage::config::UiConfig;
use lumino_core::tr;

use super::overlay_window::{ClosePolicy, OverlaySpec, OverlayWindow};

pub const PROGRESS_WINDOW_WIDTH: u32 = 500;
pub const PROGRESS_WINDOW_HEIGHT: u32 = 200;

/// 进度窗口管理器
pub struct ProgressManager {
    /// 进度接收器
    rx: mpsc::UnboundedReceiver<(String, f64)>,
    /// 当前进度
    progress: Option<(String, f64)>,
    /// 通用浮窗（进度窗生命周期）
    overlay: Option<OverlayWindow>,
}

impl ProgressManager {
    /// 创建新的进度管理器
    pub fn new() -> (Self, mpsc::UnboundedSender<(String, f64)>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                rx,
                progress: None,
                overlay: None,
            },
            tx,
        )
    }

    /// 检查是否是进度窗口的 ID
    pub fn is_progress_window(&self, window_id: WindowId) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|o| o.is_window(window_id))
    }

    /// REND-019 PR-C：重连——进度窗存活时重绑到新 device（不存在则 no-op）。
    #[allow(dead_code)]
    pub(crate) fn rebind_gpu(&mut self, epoch: lumino_gfx::GpuEpoch, gpu: &lumino_gfx::SharedGpu) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.rebind_gpu(epoch, gpu);
        }
    }

    /// 处理进度窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        overlay.handle_event(event);
        // 浮窗内部因关闭请求释放资源后，薄壳必须同步清进度：
        // 否则下一帧 update() 会误判为「有进度无窗口」而重建窗口。
        if overlay.is_closed() {
            self.progress = None;
            self.overlay = None;
        }
    }

    /// 关闭进度窗口
    pub fn close(&mut self) {
        self.progress = None;
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.close();
        }
        self.overlay = None;
    }

    /// 更新进度窗口状态
    pub fn update(&mut self, event_loop: &ActiveEventLoop, ui_config: &UiConfig) {
        if self.progress.is_some() && self.overlay.is_none() {
            // REND-020（#166）：停帧窗口内不建进度窗——`Context::new` 会拒绝在死设备
            // 上建 Context；逐帧「建窗→丢弃」还会放大窗口/交换链开销。
            if lumino_gfx::device_lost() {
                return;
            }
            let spec = OverlaySpec {
                size: dpi::LogicalSize {
                    width: PROGRESS_WINDOW_WIDTH,
                    height: PROGRESS_WINDOW_HEIGHT,
                },
                title: tr!("MIDI 处理进度").to_string(),
                resizable: false,
                buttons: Some(WindowButtons::CLOSE | WindowButtons::MINIMIZE),
                close_policy: ClosePolicy::Immediate,
                redraw_on_input: false,
                initial_redraw: false,
            };
            match OverlayWindow::create(event_loop, ui_config, &spec, None) {
                Ok(overlay) => self.overlay = Some(overlay),
                Err(e) => tracing::error!("创建进度窗口失败: {}", e),
            }
        } else if self.progress.is_none() && self.overlay.is_some() {
            self.close();
        }
    }

    /// 处理进度消息（about_to_wait 中调用）
    pub fn process_messages(&mut self, main_ui: &mut lumino_ui::Host, main_window: &Window) {
        while let Ok((msg, progress)) = self.rx.try_recv() {
            self.handle_message(msg, progress, main_ui, main_window);
        }
    }

    /// 处理单个进度消息：镜像到主窗状态栏 + 进度窗
    fn handle_message(
        &mut self,
        msg: String,
        progress: f64,
        main_ui: &mut lumino_ui::Host,
        main_window: &Window,
    ) {
        // 进度完成（>= 1.0）时，先显示完成消息再关闭窗口（下帧 update 关闭）。
        if progress >= 1.0 {
            main_ui.update_progress(Some((msg.clone(), progress)));
            if let Some(ref mut overlay) = self.overlay {
                overlay.update_progress(Some((msg, progress)));
            }
            main_window.request_redraw();
            if let Some(ref overlay) = self.overlay {
                overlay.request_redraw();
            }
            self.progress = None;
            return;
        }

        self.progress = Some((msg.clone(), progress));
        main_ui.update_progress(Some((msg.clone(), progress)));
        if let Some(ref mut overlay) = self.overlay {
            overlay.update_progress(Some((msg, progress)));
        }
        main_window.request_redraw();
        if let Some(ref overlay) = self.overlay {
            overlay.request_redraw();
        }
    }
}
