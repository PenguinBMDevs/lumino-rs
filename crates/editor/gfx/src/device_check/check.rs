//! 检测入口与适配器枚举。
//!
//! 线程模型的超时包装与廉价探测已拆到 `timeout.rs`（原文件超 400 行红线）。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lumino_core::{tr, trf};

use super::render::render_test;
use super::report::{
    GpuAdapterSummary, GpuCheckFailure, GpuCheckReport, required_backend, required_backend_name,
    required_backends,
};

/// 回退后端诊断（`GpuCheckReport::fallback_adapters`）的收集时机。
///
/// 收集需要新建 `Backends::all` 实例并枚举 DX12 / GL 适配器，实测在本机占一次全量检测
/// 耗时的 80% 以上（调研与实测数据见
/// `docs/2026-10-03-GPU检测缓存回写与回退枚举按需收集调研.md`），因此只在"人看得见"的
/// 场景收集：
///
/// - 启动门控**通过路径**：结果只落进一行 info 日志 → [`FallbackDiagnostics::Never`] /
///   [`FallbackDiagnostics::OnFailure`]（通过时不收集）；
/// - 启动门控**失败且要弹警告窗**：`failure_detail` 需要「检测到其他可用后端…可能仍可运行」
///   → [`FallbackDiagnostics::OnFailure`]；
/// - **设置页手动检测**：通过/失败都会展示并支持「复制诊断信息」→
///   [`FallbackDiagnostics::Always`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackDiagnostics {
    /// 从不收集（启动静默路径：结果只写日志）
    Never,
    /// 仅在检测失败时收集（启动失败警告窗需要回退后端提示）
    OnFailure,
    /// 始终收集（设置页手动检测）
    Always,
}

/// 执行一次无头 GPU 兼容性检测（可能阻塞，调用方应放在后台线程）。
///
/// `when` 决定是否收集回退后端诊断——**必填参数**：收集的成本是全量检测耗时的大头，
/// 调用方必须显式声明自己的场景，避免无心调用默认策略白花数百毫秒。
///
/// 调试开关（仅 debug 构建）：
/// - `LUMINO_FORCE_DEVICE_CHECK_FAIL`：强制失败（验证警告窗）；
/// - `LUMINO_DEBUG_CHECK_HANG`：模拟检测主体挂起 60 秒（验证超时与子进程强杀）。
pub fn check_gpu_support(when: FallbackDiagnostics) -> GpuCheckReport {
    puffin::profile_function!();
    let report = check_inner(when);
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

fn check_inner(when: FallbackDiagnostics) -> GpuCheckReport {
    let started = Instant::now();

    // 调试开关（仅 debug 构建）：模拟全量检测挂起，用于验证
    // 「超时 / 子进程强杀 / 主进程 UI 不被拖死」。
    // 与 `LUMINO_DEBUG_PROBE_HANG` 的分工：那个模拟启动廉价探测（枚举）挂起，
    // 本开关模拟检测主体（建实例 / 建设备 / 提交流）挂起——两者挂起点不同，
    // 前者只影响启动缓存校验，后者才会拖死主渲染。
    #[cfg(debug_assertions)]
    if std::env::var_os("LUMINO_DEBUG_CHECK_HANG").is_some() {
        tracing::warn!("LUMINO_DEBUG_CHECK_HANG 已设置：模拟全量检测挂起 60 秒");
        std::thread::sleep(Duration::from_secs(60));
    }

    if debug_force_fail_requested() {
        tracing::warn!("LUMINO_FORCE_DEVICE_CHECK_FAIL 已设置，强制返回失败报告");
        let mut report = GpuCheckReport::failed(GpuCheckFailure::RenderError(
            tr!("调试开关 LUMINO_FORCE_DEVICE_CHECK_FAIL 强制失败").to_string(),
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
                report.fallback_adapters = fallback_adapters_with(when, false, enumerate_fallback);
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
            report.fallback_adapters = fallback_adapters_with(when, false, enumerate_fallback);
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

    let passed = failure.is_none();
    let mut report = GpuCheckReport {
        passed,
        failure,
        required_backend: required_backend_name(),
        adapters,
        fallback_adapters: fallback_adapters_with(when, passed, enumerate_fallback),
        fingerprint: Some(fingerprint),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    report.adapters.sort_by_key(|a| a.fingerprint());
    report
}

/// 枚举指定后端的全部适配器摘要（廉价探测也走这里，故对兄弟模块可见）
pub(super) fn enumerate(
    instance: &wgpu::Instance,
    backends: wgpu::Backends,
) -> Vec<GpuAdapterSummary> {
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

/// 按策略决定是否收集回退后端诊断。
///
/// 枚举以闭包 `enumerate` 注入，使「通过路径**绝不**触发枚举」成为可单测断言的行为
/// （见 `device_check::tests`）——这既是本卡验收标准 1 的确定性证据，也让决策与 IO 解耦。
///
/// 生产代码只经由 [`check_inner`] 调用；`pub(crate)` 仅为同 crate 单测服务。
pub(crate) fn fallback_adapters_with<F>(
    when: FallbackDiagnostics,
    passed: bool,
    enumerate: F,
) -> Vec<GpuAdapterSummary>
where
    F: FnOnce() -> Vec<GpuAdapterSummary>,
{
    match when {
        FallbackDiagnostics::Never => {
            tracing::debug!("回退后端诊断：跳过收集（策略 Never）");
            Vec::new()
        }
        FallbackDiagnostics::OnFailure if passed => {
            tracing::debug!("回退后端诊断：跳过收集（策略 OnFailure + 本次检测通过）");
            Vec::new()
        }
        FallbackDiagnostics::OnFailure | FallbackDiagnostics::Always => {
            tracing::debug!("回退后端诊断：开始收集（策略 {when:?}, passed={passed}）");
            enumerate()
        }
    }
}

/// 取第一条已捕获的 wgpu 错误（渲染期间出错则即使回读成功也判失败）
fn captured_error(errors: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    errors
        .lock()
        .ok()
        .filter(|list| !list.is_empty())
        .map(|list| trf!("渲染期间出现 wgpu 错误: {errors}", errors = list.join("; ")))
}
