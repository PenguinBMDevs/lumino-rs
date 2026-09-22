//! 检测入口、超时封装与适配器枚举。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::render::render_test;
use super::report::{
    GpuAdapterSummary, GpuCheckFailure, GpuCheckReport, required_backend, required_backend_name,
    required_backends,
};

/// 执行一次无头 GPU 兼容性检测（可能阻塞，调用方应放在后台线程）
pub fn check_gpu_support() -> GpuCheckReport {
    puffin::profile_function!();
    let report = check_inner();
    if report.passed {
        tracing::info!(
            "GPU 兼容性检测通过: {}",
            report.detail().replace('\n', " | ")
        );
    } else {
        tracing::warn!(
            "GPU 兼容性检测未通过: {}",
            report.detail().replace('\n', " | ")
        );
    }
    report
}

/// 在独立线程中执行检测并施加超时。超时/线程异常均返回失败报告，不阻塞调用方。
pub fn run_check_with_timeout(timeout: Duration) -> GpuCheckReport {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawn_result = std::thread::Builder::new()
        .name("lumino-gpu-check".to_string())
        .spawn(move || {
            let report = check_gpu_support();
            let _ = tx.send(report);
        });
    if let Err(e) = spawn_result {
        tracing::error!("GPU 检测线程创建失败: {e}");
        return GpuCheckReport::failed(GpuCheckFailure::RenderError(format!(
            "检测线程创建失败: {e}"
        )));
    }

    match rx.recv_timeout(timeout) {
        Ok(report) => report,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!("GPU 兼容性检测超时（{timeout:?}），按失败处理");
            let mut report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
            report.duration_ms = timeout.as_millis() as u64;
            report
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            tracing::error!("GPU 检测线程意外退出（可能 panic）");
            GpuCheckReport::failed(GpuCheckFailure::RenderError("检测线程意外退出".to_string()))
        }
    }
}

/// 调试开关：强制检测失败（仅 debug 构建，便于手动验证警告窗）。
///
/// 该开关同时会禁用调用方的指纹缓存（见 `device_gate`），保证调试路径稳定可复现。
pub fn debug_force_fail_requested() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var_os("LUMINO_FORCE_DEVICE_CHECK_FAIL").is_some()
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

fn check_inner() -> GpuCheckReport {
    let started = Instant::now();

    if debug_force_fail_requested() {
        tracing::warn!("LUMINO_FORCE_DEVICE_CHECK_FAIL 已设置，强制返回失败报告");
        let mut report = GpuCheckReport::failed(GpuCheckFailure::RenderError(
            "调试开关 LUMINO_FORCE_DEVICE_CHECK_FAIL 强制失败".to_string(),
        ));
        report.duration_ms = started.elapsed().as_millis() as u64;
        return report;
    }

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: required_backends(),
        flags: crate::context::instance_flags(),
        ..Default::default()
    });
    let mut adapters = enumerate(&instance, required_backends());

    let adapter =
        match futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })) {
            Ok(adapter) => adapter,
            Err(e) => {
                tracing::warn!("未找到 {} 适配器: {e}", required_backend_name());
                let mut report = GpuCheckReport::failed(GpuCheckFailure::NoAdapter);
                adapters.sort_by_key(|a| a.fingerprint());
                report.adapters = adapters;
                report.fallback_adapters = enumerate_fallback();
                report.duration_ms = started.elapsed().as_millis() as u64;
                return report;
            }
        };

    let chosen = GpuAdapterSummary::from_info(&adapter.get_info());
    let fingerprint = chosen.fingerprint();
    if !adapters.iter().any(|a| a.fingerprint() == fingerprint) {
        adapters.push(chosen);
        adapters.sort_by_key(|a| a.fingerprint());
    }

    let adapter_limits = adapter.limits();
    let device_result =
        futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lumino-device-check"),
            required_features: adapter.features() & wgpu::Features::default(),
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: adapter_limits.max_storage_buffer_binding_size,
                max_buffer_size: adapter_limits.max_buffer_size,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }));

    let (device, queue) = match device_result {
        Ok(pair) => pair,
        Err(e) => {
            let mut report = GpuCheckReport::failed(GpuCheckFailure::DeviceRequest(e.to_string()));
            report.adapters = adapters;
            report.fallback_adapters = enumerate_fallback();
            report.fingerprint = Some(fingerprint);
            report.duration_ms = started.elapsed().as_millis() as u64;
            return report;
        }
    };

    // 自定义未捕获错误处理，避免校验错误走 wgpu 默认 panic 路径
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let errors = Arc::clone(&errors);
        device.on_uncaptured_error(Arc::new(move |err| {
            tracing::error!("GPU 检测期间 wgpu 未捕获错误: {err}");
            if let Ok(mut list) = errors.lock() {
                list.push(err.to_string());
            }
        }));
    }
    device.set_device_lost_callback(|reason, msg| {
        tracing::error!("GPU 检测期间 device lost: {reason:?} — {msg}");
    });

    let failure = match render_test(&device, &queue) {
        Ok(()) => captured_error(&errors).map(GpuCheckFailure::RenderError),
        Err(message) => Some(GpuCheckFailure::RenderError(message)),
    };

    let mut report = GpuCheckReport {
        passed: failure.is_none(),
        failure,
        required_backend: required_backend_name(),
        adapters,
        fallback_adapters: enumerate_fallback(),
        fingerprint: Some(fingerprint),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    report.adapters.sort_by_key(|a| a.fingerprint());
    report
}

/// 廉价探测（内部实现，**禁止直接调用**）：仅枚举要求后端的适配器指纹。
///
/// # ⚠️ 使用约束（务必阅读，UI-008 / #37 事故修复）
///
/// 本函数是 **阻塞式** 的：内部新建 wgpu `Instance` 并执行 `enumerate_adapters`
/// （加载 Vulkan ICD、dlopen 各驱动）。在驱动异常的设备上，该调用可能**永久挂起**。
///
/// 历史事故：启动门控曾在主线程直接调用本函数校验指纹缓存——这是绝大多数老用户
/// 每次启动的默认路径；驱动挂起时进程冻结在窗口创建之前，用户表现为「双击无反应」。
///
/// 因此本函数可见性已收紧为 `pub(crate)`，crate 外无法调用；**唯一允许的调用方式
/// 是 [`probe_adapter_fingerprints_with_timeout`]**（独立线程 + `recv_timeout`，
/// 超时按 cache miss 处理）。同 crate 内新增调用点也必须遵守：
///
/// ```ignore
/// // ❌ 禁止：调用线程（主线程/事件循环）被阻塞，驱动挂起即无限冻结
/// let fingerprints = probe_adapter_fingerprints();
///
/// // ✅ 正确：走带超时的包装（内部独立线程；超时返回 None，按 cache miss 落回全量检测）
/// let fingerprints = probe_adapter_fingerprints_with_timeout(PROBE_TIMEOUT);
/// ```
///
/// # 返回值
///
/// 要求后端的适配器指纹列表；枚举失败 / 无适配器时为空列表（空列表在缓存比对中
/// 同样按 miss 处理，与超时语义一致）。
pub(crate) fn probe_adapter_fingerprints() -> Vec<String> {
    // 调试开关（仅 debug 构建）：模拟探测挂起，用于验证
    // 「超时 → cache miss → 自动落回全量检测 → 启动在有限时间内继续」。
    #[cfg(debug_assertions)]
    if std::env::var_os("LUMINO_DEBUG_PROBE_HANG").is_some() {
        tracing::warn!("LUMINO_DEBUG_PROBE_HANG 已设置：模拟适配器探测挂起 30 秒");
        std::thread::sleep(Duration::from_secs(30));
    }

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: required_backends(),
        flags: crate::context::instance_flags(),
        ..Default::default()
    });
    enumerate(&instance, required_backends())
        .into_iter()
        .map(|adapter| adapter.fingerprint())
        .collect()
}

/// 带超时的廉价探测：独立线程执行 + `recv_timeout`（线程模型对齐
/// [`run_check_with_timeout`]）。**这是 crate 外唯一允许的廉价探测入口。**
///
/// # 语义（UI-008 / #37）
///
/// - 返回 `Some(fingerprints)`：探测在超时内完成（列表可能为空）；
/// - 返回 `None`：超时 / 探测线程 panic / 探测线程创建失败。调用方必须按
///   **cache miss** 处理，落回全量检测——**不得**判定为「检测失败」，更不得弹
///   警告窗（探测没回来只说明缓存无法快速校验，不代表 GPU 不受支持）；
/// - 全路径 tracing 日志：开始 / 完成（含耗时与适配器数）/ 超时落回。
///
/// # 用法
///
/// ```ignore
/// // 启动缓存校验（device_gate）：
/// let cache_hit = device_check_policy::cache_hit(
///     within_ttl, force_fail, cached_passed, cached_fp.as_deref(),
///     || probe_adapter_fingerprints_with_timeout(PROBE_TIMEOUT).unwrap_or_default(),
/// );
/// // cache_hit == false → 调用方执行 run_check_with_timeout(DEFAULT_TIMEOUT) 全量检测
/// ```
///
/// # 最坏耗时
///
/// 最多阻塞 `timeout`（默认 [`crate::device_check::PROBE_TIMEOUT`] = 1.5 秒），
/// 取代修复前的「无限冻结」；配合全量检测 3 秒超时，启动最坏阻塞 ≈ 4.5 秒。
///
/// # 已知遗留（本卡不处理，记录在案）
///
/// 超时后探测线程处于 detach 状态、无法终止（Rust 无线程取消），极端情况下
/// 可能与随后的全量检测线程并存创建 wgpu `Instance`；后续另开任务评估线程收敛。
pub fn probe_adapter_fingerprints_with_timeout(timeout: Duration) -> Option<Vec<String>> {
    probe_with_timeout(timeout, probe_adapter_fingerprints)
}

/// [`probe_adapter_fingerprints_with_timeout`] 的可注入测试核心。
///
/// `probe` 闭包在独立线程中执行；超时 / 线程 panic / 线程创建失败一律返回 `None`
/// （调用方按 cache miss 处理）。生产代码只应通过上面的公开包装调用。
pub(crate) fn probe_with_timeout<F>(timeout: Duration, probe: F) -> Option<Vec<String>>
where
    F: FnOnce() -> Vec<String> + Send + 'static,
{
    tracing::info!("GPU 适配器廉价探测：开始（超时 {timeout:?}）");
    let started = Instant::now();

    let (tx, rx) = std::sync::mpsc::channel();
    let spawn_result = std::thread::Builder::new()
        .name("lumino-gpu-probe".to_string())
        .spawn(move || {
            let fingerprints = probe();
            let _ = tx.send(fingerprints);
        });
    if let Err(e) = spawn_result {
        tracing::warn!("GPU 适配器廉价探测：探测线程创建失败（{e}），按 cache miss 落回全量检测");
        return None;
    }

    match rx.recv_timeout(timeout) {
        Ok(fingerprints) => {
            tracing::info!(
                "GPU 适配器廉价探测：完成（{}ms，{} 个适配器）",
                started.elapsed().as_millis(),
                fingerprints.len()
            );
            Some(fingerprints)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!(
                "GPU 适配器廉价探测：超时（{}ms / 上限 {timeout:?}），按 cache miss 落回全量检测",
                started.elapsed().as_millis()
            );
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            tracing::warn!(
                "GPU 适配器廉价探测：探测线程异常退出（可能 panic），按 cache miss 落回全量检测"
            );
            None
        }
    }
}

/// 枚举指定后端的全部适配器摘要
fn enumerate(instance: &wgpu::Instance, backends: wgpu::Backends) -> Vec<GpuAdapterSummary> {
    instance
        .enumerate_adapters(backends)
        .iter()
        .map(|adapter| GpuAdapterSummary::from_info(&adapter.get_info()))
        .collect()
}

/// 枚举非要求后端的适配器（仅诊断用途，不创建设备）
fn enumerate_fallback() -> Vec<GpuAdapterSummary> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: crate::context::instance_flags(),
        ..Default::default()
    });
    let required = required_backend();
    instance
        .enumerate_adapters(wgpu::Backends::all())
        .into_iter()
        .filter(|adapter| adapter.get_info().backend != required)
        .map(|adapter| GpuAdapterSummary::from_info(&adapter.get_info()))
        .collect()
}

/// 取第一条已捕获的 wgpu 错误（渲染期间出错则即使回读成功也判失败）
fn captured_error(errors: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    errors
        .lock()
        .ok()
        .filter(|list| !list.is_empty())
        .map(|list| format!("渲染期间出现 wgpu 错误: {}", list.join("; ")))
}
