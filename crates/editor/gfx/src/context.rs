use std::sync::{Arc, RwLock};
use thiserror::Error;

use crate::device_status;
use crate::epoch::GpuEpoch;

// REND-020（#166）：构造路径拆到 `context/create.rs`，运行时操作在
// `context/ops.rs`——主文件守住 400 行红线（红线 10），行为不变。
mod create;
mod ops;

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;

/// 交换链纹理用途。
///
/// `COPY_DST` **不可省**（2026-10-19 Android 真机实测回归）：钢琴卷帘/瀑布流由
/// 分离渲染线程画进离屏纹理，再经 `copy_offscreen_to_surface` 的
/// `copy_texture_to_texture` 拷回交换链（`render_thread/thread/blit.rs`）。
/// 目标纹理缺少 `COPY_DST` 时该拷贝被 wgpu 校验拒绝——表现是 UI 框架正常、
/// **音符区整块黑**。回归测试见 `context/tests.rs`。
///
/// REND-020 拆分后本函数留在主文件：`create.rs`（配置）与 `tests.rs`（断言）
/// 都引用它，放在父模块是二者唯一的公共落点。
#[must_use]
pub(crate) fn surface_usage() -> wgpu::TextureUsages {
    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST
}

/// 进程级共享的 wgpu 资源
///
/// 多窗口场景下重复创建 Instance/Adapter/Device 是启动瓶颈之一，
/// 因此把这些重量级资源提升到进程级，仅每个窗口保留自己的 Surface。
pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// 进程级共享 GPU 状态（REND-019 PR-B）。
///
/// 重连时整体替换 `inner` 并递增 `epoch`（PR-C）——旧代次的一切跨帧资源
/// 依据 [`GpuEpoch`] 判废并走既有重建路径（E2/E3），禁止跨代次复用。
#[derive(Clone)]
pub struct SharedGpuState {
    /// 当前 GPU 代次（每次 device 重建单调递增）
    pub epoch: GpuEpoch,
    /// 共享 GPU 资源（instance / adapter / device / queue 结构不变）
    pub inner: Arc<SharedGpu>,
}

/// 唯一全局入口（REND-019 PR-B，G1）。
///
/// - device/queue 只能经 [`gpu_snapshot`] 或调用方传入的
///   `(GpuEpoch, Arc<SharedGpu>)` 获取；禁止新增长期存放 `Arc<SharedGpu>`
///   的 `OnceLock` / `static`（既有 SHARED_ENGINE / blit CACHE 属改造对象，非绕过对象）；
/// - 读锁拿快照后立即放锁——禁止持锁做任何 wgpu 调用 / `.await` / 阻塞收发（G3）。
static SHARED_GPU: RwLock<Option<std::result::Result<SharedGpuState, String>>> = RwLock::new(None);

/// 根据构建配置决定 wgpu 实例 flags
///
/// Debug 构建保留校验层便于排错；Release 构建关闭所有校验，
/// 避免 device 创建和运行时产生额外开销。
pub(crate) fn instance_flags() -> wgpu::InstanceFlags {
    #[cfg(debug_assertions)]
    {
        wgpu::InstanceFlags::DEBUG
            | wgpu::InstanceFlags::VALIDATION
            | wgpu::InstanceFlags::GPU_BASED_VALIDATION
    }

    #[cfg(not(debug_assertions))]
    {
        wgpu::InstanceFlags::empty()
    }
}

/// 按统一口径为 adapter 构建 device/queue（初建与重连共用，避免特征/限制漂移）。
async fn build_device_queue(
    adapter: &wgpu::Adapter,
) -> std::result::Result<(wgpu::Device, wgpu::Queue), ContextError> {
    let adapter_features = adapter.features();
    let limits = adapter.limits();

    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: adapter_features & wgpu::Features::default(),
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
                max_buffer_size: limits.max_buffer_size,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .map_err(|e| ContextError::DeviceRequest(e.to_string()))
}

/// 图形上下文创建与帧获取阶段可能发生的错误
#[derive(Error, Debug)]
pub enum ContextError {
    /// 创建 `wgpu::Surface` 失败
    #[error("{}: {}", lumino_core::i18n::tr("创建 surface 失败"), .0)]
    SurfaceCreation(String),
    /// 创建 `wgpu::Adapter` 失败
    #[error("{}: {}", lumino_core::i18n::tr("创建 adapter 失败"), .0)]
    AdapterCreation(String),
    /// 未找到可用的 preferred format
    #[error("{}", lumino_core::i18n::tr("获取 preferred format 失败"))]
    PreferredFormatNotFound,
    /// 请求 `wgpu::Device` 失败
    #[error("{}: {}", lumino_core::i18n::tr("请求 device 失败"), .0)]
    DeviceRequest(String),
    /// GPU 共享资源尚未初始化（`gpu_snapshot` 在初始化完成前被调用）
    #[error("{}", lumino_core::i18n::tr("GPU 尚未初始化"))]
    GpuUninitialized,
    /// 设备已丢失、等待重连（REND-020：停帧窗口内禁止创建依赖死设备的上下文）
    #[error("{}", lumino_core::i18n::tr("GPU 设备已丢失（等待重连）"))]
    DeviceLost,
    /// 获取交换链帧失败（含 `wgpu::SurfaceError`）
    #[error("{}: {}", lumino_core::i18n::tr("获取帧失败"), .0)]
    FrameAcquisition(#[from] wgpu::SurfaceError),
}

/// 图形上下文操作的通用结果类型
pub type Result<T> = std::result::Result<T, ContextError>;

/// 获取当前 GPU 代次快照（REND-019 PR-B，G1：唯一允许的读取方式）。
///
/// 读锁拿快照后立即放锁——禁止持锁做任何 wgpu 调用 / `.await` / 阻塞收发（G3）。
/// 返回的 `Arc<SharedGpu>` 只允许在「一次性渲染 / 上传动作」内使用（G2）；
/// 跨帧持有者必须连 [`GpuEpoch`] 一起存，且每帧使用前比对。
///
/// # 错误
/// - [`ContextError::GpuUninitialized`]：共享 GPU 尚未初始化
/// - [`ContextError::AdapterCreation`]：初始化曾失败（透传当时存储的错误串）
pub fn gpu_snapshot() -> Result<(GpuEpoch, Arc<SharedGpu>)> {
    let guard = SHARED_GPU
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match guard.as_ref() {
        Some(Ok(state)) => Ok((state.epoch, Arc::clone(&state.inner))),
        Some(Err(e)) => Err(ContextError::AdapterCreation(e.clone())),
        None => Err(ContextError::GpuUninitialized),
    }
}

/// REND-019 PR-C：重连 worker——从旧共享状态重建 adapter/device（在专用线程执行，
/// 禁止在 UI 线程同步等待；结果经 channel 交回 UI 主循环安装）。
///
/// 复用同一 `Instance`：各窗口 `Surface` 挂在其中——换 instance 需把全部窗口
/// surface 重建；同 instance 下由各窗口 `rebind_device` 重配即可恢复（E2）。
pub fn recreate_shared_gpu_worker() -> std::result::Result<(GpuEpoch, SharedGpu), String> {
    let (old_epoch, old) = gpu_snapshot().map_err(|e| e.to_string())?;
    let instance = old.instance.clone();

    // 适配器选择沿用初建口径（env 覆盖 + 默认偏好）；无 surface 做兼容性检查
    let adapter = futures::executor::block_on(wgpu::util::initialize_adapter_from_env_or_default(
        &instance, None,
    ))
    .map_err(|e| format!("重连初始化适配器失败: {e}"))?;

    let (device, queue) = futures::executor::block_on(build_device_queue(&adapter))
        .map_err(|e| format!("重连请求设备失败: {e}"))?;

    // 回调接线与初建口径一致（uncaptured + device-lost 置位/通知）
    device.on_uncaptured_error(Arc::new(|err| {
        tracing::error!("wgpu uncaptured error: {err}")
    }));
    device.set_device_lost_callback(device_status::on_device_lost);

    Ok((
        old_epoch.next(),
        SharedGpu {
            instance,
            adapter,
            device,
            queue,
        },
    ))
}

/// REND-019 PR-C：安装重连产物为新的共享 GPU 状态（epoch 单调递增）。
///
/// 只替换 `SHARED_GPU` 槽位；停帧标志由重连流水线在基础设施重建完成后撤销。
pub fn install_recreated_shared_gpu(
    epoch: GpuEpoch,
    gpu: SharedGpu,
) -> std::result::Result<(), String> {
    // 写锁内只做赋值（G3：无 wgpu 调用）
    let mut guard = SHARED_GPU
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(Ok(SharedGpuState {
        epoch,
        inner: Arc::new(gpu),
    }));
    tracing::info!("共享 GPU 已重建并安装：epoch = {}", epoch.0);
    Ok(())
}

/// REND-019 PR-C（R5-b 加固）：重建 `Surface` 的工厂。
///
/// 重连时必须**整体替换** Surface：wgpu-core 的 `device_destroy` 只置
/// `valid=false`，**不清理**关联 surface 的 presentation / hal swapchain——
/// 旧 swapchain 会残留。若直接对新 device `configure`，wgpu-hal 会拿旧 device
/// 已销毁的 swapchain 句柄、用新 device 的 ash 函数指针去
/// `vkCreateSwapchainKHR(oldSwapchain)` + `vkDestroySwapchainKHR`（跨设备句柄
/// 使用=UB）。实测第二次重连在对话框窗口首次 configure 访问违例（0xC0000005）。
///
/// 用工厂重建 Surface 后，旧 Surface 的 Drop 由 core 走
/// `unconfigure(present.device.raw())`——用**归属它的旧 device** 正点销毁旧
/// swapchain（调用方以旧 device 克隆为存活锚点保证其尚未析构）。
pub type SurfaceFactory = Box<
    dyn Fn(&wgpu::Instance) -> std::result::Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError>
        + Send
        + Sync,
>;

/// GPU 渲染上下文，封装 surface / adapter / device / queue 等 wgpu 资源
pub struct Context {
    /// 渲染目标 surface（窗口或离屏）
    pub surface: wgpu::Surface<'static>,
    /// 图形适配器（GPU 设备句柄）
    pub adapter: wgpu::Adapter,
    /// 命令队列（提交命令缓冲）
    pub queue: wgpu::Queue,
    /// 逻辑 GPU 设备（创建管线与缓冲）
    pub device: wgpu::Device,
    /// 当前 surface 纹理格式（优先 sRGB）
    pub format: wgpu::TextureFormat,
    /// 当前交换链配置（REND-019 PR-A：`resize` 与 `handle_frame_error` 的
    /// Outdated 重配共用同一份，避免两处拼装 `SurfaceConfiguration` 漂移）
    surface_config: wgpu::SurfaceConfiguration,
    /// 本上下文所属 GPU 代次（REND-019 PR-B：跨帧资源比对基准；PR-C 重连后
    /// 旧上下文整体丢弃、由新代次重建。读取走 `Context::epoch()`）
    epoch: GpuEpoch,
    /// 重建 Surface 的工厂（REND-019 R5-b 加固；见 [`SurfaceFactory`] 与
    /// `Context::rebind_device` 说明）
    surface_factory: SurfaceFactory,
}

// `Context::new` 的构造路径在 `context/create.rs`（REND-020 拆分，行为不变）；
// 运行时操作（`epoch` / `resize` / `with_frame` / `handle_frame_error`）在 `ops.rs`。
