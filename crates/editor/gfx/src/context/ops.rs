//! `Context` 运行时操作（REND-019 PR-B 自 context.rs 拆分）。
//!
//! 拆分动机：主文件保持 400 行红线内（红线 10「文件超 400 行必须拆分」，
//! 错误处理/运行时操作分文件放置）；`context/ops.rs` 是 `context` 模块的
//! 子模块，可直接访问其私有字段（`surface_config`），拆分后逻辑与拆分前
//! 完全一致（行为不变）。
//!
//! 包含：代次读取（`epoch`）/ 同步构造（`new_blocking`）/ 交换链 resize /
//! 帧获取（`with_frame`）/ 帧错误处置（`handle_frame_error`）。

use crate::ContextError;
use crate::context::{Context, Result};
use crate::device_status;
use crate::epoch::GpuEpoch;
use crate::frame_error::{FrameErrorAction, classify_frame_error};

impl Context {
    /// 本上下文所属 GPU 代次（REND-019 PR-B：跨帧资源比对基准）。
    #[must_use]
    pub fn epoch(&self) -> GpuEpoch {
        self.epoch
    }

    /// REND-019 PR-C：重连后把本上下文重绑到新 device。
    ///
    /// Surface 挂在同一 `Instance` 上（重连复用 instance），无需重建——
    /// 换 device/queue 克隆 + 按既有交换链配置重配即可恢复（E2）。
    pub fn rebind_device(&mut self, epoch: GpuEpoch, gpu: &crate::SharedGpu) {
        self.adapter = gpu.adapter.clone();
        self.device = gpu.device.clone();
        self.queue = gpu.queue.clone();
        self.epoch = epoch;
        self.surface.configure(&self.device, &self.surface_config);
        tracing::info!("Context 已重绑到新 device：epoch = {}", epoch.0);
    }

    /// 同步创建图形上下文（在无法使用 async 的初始化路径中使用）
    ///
    /// 内部使用 `futures::executor::block_on` 驱动异步初始化流程，
    /// 避免在多个窗口管理器中重复书写 block_on 样板。
    pub fn new_blocking(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        futures::executor::block_on(Self::new(target, width, height))
    }

    /// 调整交换链尺寸并重新配置 surface。
    ///
    /// # 参数
    /// * `width` — 新宽度（像素）
    /// * `height` — 新高度（像素；任一为 0 时直接忽略本次调整）
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let max_dim = self.device.limits().max_texture_dimension_2d;
        // REND-019（PR-A）：就地更新保存的交换链配置——与
        // `handle_frame_error` 的 Outdated 重配共用同一份（视图格式 /
        // 呈现模式等不再两处重复拼装）。
        self.surface_config.width = width.min(max_dim);
        self.surface_config.height = height.min(max_dim);
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// 获取当前交换链帧并在其上执行一次绘制。
    ///
    /// 若获取帧失败（如 OutOfMemory）则返回对应错误而不调用回调。
    ///
    /// # 参数
    /// * `f` — 接收帧纹理与视图的回调，在其中提交绘制命令
    ///
    /// # 返回值
    /// * `Ok(())` — 回调执行成功
    /// * `Err(ContextError::FrameAcquisition)` — 获取帧失败；REND-019（PR-A）
    ///   起调用方必须经 [`Context::handle_frame_error`] 分类处置，禁止盲重绘
    pub fn with_frame(
        &self,
        f: impl FnOnce(&wgpu::SurfaceTexture, &wgpu::TextureView),
    ) -> Result<()> {
        puffin::profile_function!();

        // 两个真正的 GPU 同步点：获取交换链帧（可能在 GPU 忙时阻塞）与 present
        // （GPU 积压时阻塞）。单独插桩以便在火焰图中区分「取帧等待」与「呈现等待」，
        // 定位播放期突发性大卡顿究竟来自哪一环。
        let frame = {
            puffin::profile_scope!("get_current_texture");
            match self.surface.get_current_texture() {
                Ok(frame) => frame,
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    tracing::error!("Swapchain error: OutOfMemory. Rendering cannot continue.");
                    return Err(ContextError::FrameAcquisition(
                        wgpu::SurfaceError::OutOfMemory,
                    ));
                }
                Err(e) => return Err(ContextError::FrameAcquisition(e)),
            }
        };

        let view = {
            puffin::profile_scope!("create_view");
            frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        };

        f(&frame, &view);

        // REND-019 PR-C 硬化（R2 冒烟实测崩溃根因）：
        // 对已丢失设备 present 或 discard 均走 wgpu `handle_error_fatal` 内部 panic
        // （"Parent device is lost"，绕不过未捕获错误处理器）；若 panic 向上展开，
        // 外层（Runner 析构）的 Surface 信号量校验会二次 panic → abort。
        // 此处**局部** catch_unwind 截断展开：命中即按设备丢失处理（置停帧标志 +
        // 返回 Lost 错误，交调用方 `handle_frame_error` → Stop，既有连接管）。
        // 依据（wgpu 27.0.1 `api/surface_texture.rs`）：`present(mut self)` 先置
        // `presented = true` 再调用 fatal 路径——展开时 Drop 跳过 discard；
        // 局部截断保证不会有二次 panic 逃逸。
        // 说明：release（panic = "abort"）下本防护失效——见 PR-C 实施记录「已知限制」。
        let present_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            {
                puffin::profile_scope!("present_poll_lost_check");
                // poll 在轮询阶段**同步送达** device-lost 回调（R1 实测不变量：
                // destroy 后单次 poll 即置位停帧标志；poll 自身返回 Ok，不作依据）——
                // 标志命中则丢弃本帧、跳过 present，交重连接管。
                let _ = self.device.poll(wgpu::PollType::Poll);
            }
            if device_status::device_lost() {
                tracing::warn!("with_frame：设备已丢失，跳过 present（本帧丢弃，交重连接管）");
                drop(frame); // 可能触发 discard fatal → 由外层 catch 截断
                return;
            }
            {
                puffin::profile_scope!("present");
                frame.present(); // 对丢失设备 fatal → 由外层 catch 截断
            }
        }));
        if let Err(payload) = present_outcome {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "未知 panic 载荷".to_string());
            tracing::error!(
                "with_frame：present/discard 触发 wgpu fatal panic（设备丢失在途）——按设备丢失处理: {detail}"
            );
            device_status::mark_device_lost();
            return Err(ContextError::FrameAcquisition(wgpu::SurfaceError::Lost));
        }

        Ok(())
    }

    /// REND-019（PR-A §1.6）：统一帧错误处置入口——5 处 `with_frame`
    /// 调用点收口，替代原先对 `Err` 一律盲 `request_redraw` 的重试。
    ///
    /// 动作语义见 [`FrameErrorAction`]：
    /// - `Outdated` → 按保存的交换链配置重新 configure（`Reconfigured`）；
    /// - `Timeout` → 本帧放弃（`SkipFrame`）；
    /// - `Lost` / `OutOfMemory` → 置停帧标志等重连（`Stop`，禁止盲重绘）。
    ///
    /// 调用方约定：`action.should_redraw()` 为 true 时安排一次重绘。
    ///
    /// （REND-019 PR-B：自 context.rs 主文件搬移至此；逻辑与 PR-A 完全一致。）
    #[must_use]
    pub fn handle_frame_error(&self, err: &ContextError) -> FrameErrorAction {
        let ContextError::FrameAcquisition(surface_err) = err else {
            // with_frame 只产出 FrameAcquisition；其他变体按本帧放弃处理
            tracing::warn!("handle_frame_error：非帧获取类错误按本帧放弃处理: {err}");
            return FrameErrorAction::SkipFrame;
        };
        let action = classify_frame_error(surface_err);
        match action {
            FrameErrorAction::Reconfigured => {
                tracing::warn!("Surface Outdated：按现有配置重新 configure 交换链");
                self.surface.configure(&self.device, &self.surface_config);
            }
            FrameErrorAction::SkipFrame => {
                tracing::debug!("Surface Timeout：本帧放弃，下一帧重试");
            }
            FrameErrorAction::Stop => {
                tracing::error!("Surface {surface_err:?}：置停帧标志，等待重连接管（禁止盲重绘）");
                device_status::mark_device_lost();
            }
        }
        action
    }
}
