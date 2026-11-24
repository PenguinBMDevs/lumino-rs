//! 云传输（上传/下载）进度悬浮窗口管理器（薄壳）
//!
//! 窗口生命周期由 [`super::overlay_window::OverlayWindow`] 统一实现；
//! 与 `ProgressManager` 的差异仅为规格（尺寸/标题/按钮）与锚点定位：
//! - **覆盖型悬浮定位**：窗口创建时定位到锚点窗口（云浏览对话框）中心，
//!   无云浏览对话框时回退到主窗口中心
//! - 云上传/下载为阻塞调用无精确进度回调，采用阶段进度（0.1 开始 → 1.0 结束）

use tokio::sync::mpsc;
use winit::{
    dpi, event::WindowEvent, event_loop::ActiveEventLoop, window::Window, window::WindowButtons,
    window::WindowId,
};

use lumino_core::storage::config::UiConfig;
use lumino_core::tr;

use super::overlay_window::{ClosePolicy, OverlaySpec, OverlayWindow};

/// 云进度悬浮窗口尺寸（比 MIDI 进度窗更小巧，悬浮感更强）
pub const CLOUD_PROGRESS_WIDTH: u32 = 400;
pub const CLOUD_PROGRESS_HEIGHT: u32 = 160;

/// 云传输进度悬浮窗口管理器
pub struct CloudProgressManager {
    /// 进度接收器
    rx: mpsc::UnboundedReceiver<(String, f64)>,
    /// 当前进度
    progress: Option<(String, f64)>,
    /// 通用浮窗（云进度窗生命周期）
    overlay: Option<OverlayWindow>,
}

impl CloudProgressManager {
    /// 创建管理器（返回发送端供后台线程推送进度）
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

    /// 检查是否是云进度悬浮窗口的 ID
    pub fn is_cloud_progress_window(&self, window_id: WindowId) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|o| o.is_window(window_id))
    }

    /// REND-019 PR-C：重连——云进度窗存活时重绑到新 device（不存在则 no-op）。
    #[allow(dead_code)]
    pub(crate) fn rebind_gpu(&mut self, epoch: lumino_gfx::GpuEpoch, gpu: &lumino_gfx::SharedGpu) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.rebind_gpu(epoch, gpu);
        }
    }

    /// 处理悬浮窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        overlay.handle_event(event);
        // 浮窗内部关闭后同步清进度，避免下一帧误判重建（与进度窗一致）。
        if overlay.is_closed() {
            self.progress = None;
            self.overlay = None;
        }
    }

    /// 关闭悬浮窗口
    pub fn close(&mut self) {
        self.progress = None;
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.close();
        }
        self.overlay = None;
    }

    /// 更新悬浮窗口状态（about_to_wait 中调用）
    ///
    /// `anchor` 为覆盖定位的锚点窗口：云浏览对话框（有则用）或主窗口。
    /// 调用方恒能取到窗口（无对话框时回退主窗），故用非 `Option` 引用在类型
    /// 层面消除「永远走不到 `None`」的死路径。
    pub fn update(&mut self, event_loop: &ActiveEventLoop, ui_config: &UiConfig, anchor: &Window) {
        if self.progress.is_some() && self.overlay.is_none() {
            // REND-020（#166）：停帧窗口内不建云进度浮窗
            if lumino_gfx::device_lost() {
                return;
            }
            let spec = OverlaySpec {
                size: dpi::LogicalSize {
                    width: CLOUD_PROGRESS_WIDTH,
                    height: CLOUD_PROGRESS_HEIGHT,
                },
                title: tr!("云存储传输").to_string(),
                // 与 MIDI 进度窗一致：固定布局进度浮窗不可缩放，仅保留关闭/最小化。
                // 原实现注释称「不可缩放」却从未调用 with_resizable(false)，
                // 本卡按注释语义拍板（步骤5）。
                resizable: false,
                buttons: Some(WindowButtons::CLOSE | WindowButtons::MINIMIZE),
                close_policy: ClosePolicy::Immediate,
            };
            match OverlayWindow::create(event_loop, ui_config, &spec, Some(anchor)) {
                Ok(overlay) => self.overlay = Some(overlay),
                Err(e) => tracing::error!("创建云进度悬浮窗失败: {}", e),
            }
        } else if self.progress.is_none() && self.overlay.is_some() {
            self.close();
        }
    }

    /// 处理进度消息（about_to_wait 中调用）
    pub fn process_messages(&mut self) {
        while let Ok((msg, progress)) = self.rx.try_recv() {
            self.handle_message(msg, progress);
        }
    }

    /// 处理单条进度消息：进度 >= 1.0 时关闭悬浮窗
    fn handle_message(&mut self, msg: String, progress: f64) {
        if progress >= 1.0 {
            // 先刷新一次最终状态，再关闭窗口（下帧 update 关闭）
            if let Some(ref mut overlay) = self.overlay {
                overlay.update_progress(Some((msg, progress)));
            }
            if let Some(ref overlay) = self.overlay {
                overlay.request_redraw();
            }
            self.progress = None;
            return;
        }

        self.progress = Some((msg.clone(), progress));
        if let Some(ref mut overlay) = self.overlay {
            overlay.update_progress(Some((msg, progress)));
        }
        if let Some(ref overlay) = self.overlay {
            overlay.request_redraw();
        }
    }
}
