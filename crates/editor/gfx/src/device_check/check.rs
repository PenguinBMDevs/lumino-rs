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

/// 廉价探测：仅枚举要求后端的适配器指纹（不创建设备，用于启动缓存比对）
pub fn probe_adapter_fingerprints() -> Vec<String> {
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
