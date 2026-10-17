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
    ///
    /// ⚠️ **configure 必须调用两次**（两处均为有意为之，禁止「优化」成一次；
    /// R2 fast-release 冒烟 2026-10-16 实测）：TDR 时若有帧在途，「跳过
    /// present」后帧对象 Drop 走 `discard()`，而 wgpu-core-27.0.3
    /// `Surface::discard`（`present.rs:333-338`）在 `device.check_is_valid()`
    /// （设备已丢）处提前失败——已取得的 `acquired_texture` **永不被释放**。
    /// 此时首次 `configure` 在 `surface_configure`（`device/global.rs:2021-2026`）
    /// 撞上残留报 `PreviousOutputExists`，且该路径**已先 `take()` 掉旧
    /// presentation**：失败后 surface 进入「无 presentation」永久未配置态 →
    /// 此后每帧 `get_current_texture` 报 `NotConfigured`（错误走 uncaptured
    /// handler），调用方按本帧放弃重试 → 约 6kHz 空转刷日志、渲染永不恢复。
    /// 第二次 `configure` 时残留已随 `take()` 一并清除，必然成功安装新配置；
    /// 首配无残留时第二次为幂等重配（重连低频路径，代价可忽略）。
    pub fn rebind_device(&mut self, epoch: GpuEpoch, gpu: &crate::SharedGpu) {
        // REND-019（R5-b 加固）：**先整体替换 Surface，再换 device**。
        //
        // wgpu-core 的 `device_destroy` 只置 `valid=false`，不清理关联
        // surface 的 presentation / hal swapchain——旧 swapchain 残留在 hal
        // Surface 内。若直接对新 device `configure`，wgpu-hal 的
        // `create_swapchain` 会拿旧 device 已销毁的 swapchain 句柄、用新
        // device 的 ash 函数指针去 `vkCreateSwapchainKHR(oldSwapchain)` +
        // `vkDestroySwapchainKHR`（跨设备句柄使用=UB）。实测第二次重连在
        // 对话框窗口的首次 configure 访问违例（0xC0000005）。
        //
        // 替换 Surface：旧 Surface Drop 走 core `unconfigure(present.device
        // .raw())`，用归属它的旧 device 正点销毁旧 swapchain；`old_device`
        // 克隆作为存活锚点，保证 unconfigure 期间旧 VkDevice 尚未析构。
        let old_device = self.device.clone();
        self.replace_surface(&gpu.instance);
        drop(old_device);

        self.adapter = gpu.adapter.clone();
        self.device = gpu.device.clone();
        self.queue = gpu.queue.clone();
        self.epoch = epoch;
        // 两次 configure：替换后 surface 为全新对象（无 presentation），首配
        // 理论必然走安装路径；双配保留为防御（替换失败回退路径 / 平台差异）。
        tracing::info!("Context 重绑：交换链重配开始（epoch = {}）", epoch.0);
        self.configure_surface_stable();
        tracing::info!("Context 已重绑到新 device：epoch = {}", epoch.0);
    }

    /// REND-019（R5-b 加固）：重建本上下文的 `Surface`（语义见 `rebind_device`）。
    ///
    /// 失败（如窗口已销毁）时保留旧 Surface——退化为旧行为，由随后的
    /// configure 兜底（存在跨设备句柄风险，但总好于直接丢弃窗口渲染）。
    fn replace_surface(&mut self, instance: &wgpu::Instance) {
        match (self.surface_factory)(instance) {
            Ok(surface) => {
                // 赋值瞬间旧 Surface Drop：core 经旧 device 正点 unconfigure
                //（调用方以 old_device 克隆保证旧 device 存活，见 rebind_device）
                self.surface = surface;
            }
            Err(e) => {
                tracing::error!("重连：重建窗口 Surface 失败（保留旧 Surface 兜底）：{e}");
            }
        }
    }

    /// 同步创建图形上下文（在无法使用 async 的初始化路径中使用）
    ///
    /// 内部使用 `futures::executor::block_on` 驱动异步初始化流程，
    /// 避免在多个窗口管理器中重复书写 block_on 样板。
    pub fn new_blocking(
        surface_factory: impl Fn(
            &wgpu::Instance,
        ) -> std::result::Result<
            wgpu::Surface<'static>,
            wgpu::CreateSurfaceError,
        > + Send
        + Sync
        + 'static,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        futures::executor::block_on(Self::new(surface_factory, width, height))
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
        // REND-019（R2 实测 2026-10-17）：停帧期间（设备已销毁、等待重连）禁止
        // 重新 configure——对已销毁设备的 surface.configure 会走
        // maintain(wait_indefinitely) 等待永不完成的 GPU 工作，UI 线程永久卡死。
        if device_status::device_lost() {
            return;
        }
        let max_dim = self.device.limits().max_texture_dimension_2d;
        // REND-019（PR-A）：就地更新保存的交换链配置——与
        // `handle_frame_error` 的 Outdated 重配共用同一份（视图格式 /
        // 呈现模式等不再两处重复拼装）。
        self.surface_config.width = width.min(max_dim);
        self.surface_config.height = height.min(max_dim);
        self.configure_surface_stable();
    }

    /// 稳健重配交换链（REND-019，R5-b 实测补强）：**连续 configure 两次**。
    ///
    /// wgpu-core 的 `Surface::configure` 失败路径（`PreviousOutputExists` /
    /// 底层 `Invalid surface`）会**先把旧 presentation `take()` 走**——一次失败
    /// 即 surface 进入「无 presentation」态，后续每帧 `get_current_texture`
    /// 报 `NotConfigured` 形成热循环（R5-b：恢复后微扰重配一次失败 → 25 万行
    /// 洪泛 → 进程访问违例）。第二次 configure 时残留已被清掉，可正常安装；
    /// 首配成功时第二次为幂等重配（低频路径，代价可忽略）。
    /// 禁止「优化」为单次（同 `rebind_device` 的双 configure 语义）。
    fn configure_surface_stable(&self) {
        self.surface.configure(&self.device, &self.surface_config);
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

        // REND-019（R2 实测 2026-10-17 第 2 轮卡死修复）：
        // 1) 帧首先 poll：送达可能已发生的 device-lost 回调（R1 不变量：poll
        //    同步送达）并检查停帧标志——**禁止在已销毁设备上执行
        //    `get_current_texture`**：release（无校验层）下该调用会驱动级阻塞，
        //    UI 线程永久卡死且回调永远送不出（R2 第 2 轮实测：注入后 24s 零日志）。
        // 2) R2 注入安全点：注入请求在此处（帧间静态点、本线程无 surface 调用
        //    在途）执行 destroy，避免注入线程与在途 acquire/present 竞态。
        let _ = self.device.poll(wgpu::PollType::Poll);
        if device_status::take_injection_request() {
            tracing::warn!(
                "[gpu-reconnect-test] 注入 device.destroy()（UI 帧安全点，R2，与真实 TDR 同回调路径）"
            );
            self.device.destroy();
            // destroy 后立即 poll：同步送达 lost 回调 → 置停帧标志（R1 不变量）
            let _ = self.device.poll(wgpu::PollType::Poll);
        }
        if device_status::device_lost() {
            return Err(ContextError::FrameAcquisition(wgpu::SurfaceError::Lost));
        }

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
            let _ = device_status::mark_device_lost();
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
                self.configure_surface_stable();
            }
            FrameErrorAction::SkipFrame => {
                tracing::debug!("Surface Timeout：本帧放弃，下一帧重试");
            }
            FrameErrorAction::Stop => {
                // REND-019（R2 实测）：停帧窗口内重绘循环每帧都会走到本分支——
                // 仅首次置位时告警（此前每帧一条 ERROR，2.3s 窗口实测 5594 条）。
                if device_status::mark_device_lost() {
                    tracing::error!(
                        "Surface {surface_err:?}：置停帧标志，等待重连接管（禁止盲重绘）"
                    );
                } else {
                    tracing::debug!(
                        "Surface {surface_err:?}：停帧标志已置位（本帧放弃，等待重连接管）"
                    );
                }
            }
        }
        action
    }
}
