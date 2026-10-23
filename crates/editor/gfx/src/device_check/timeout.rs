//! 线程模型的超时包装与廉价探测。
//!
//! 从 `check.rs` 拆出（原文件 419 行超 400 红线）。本模块只做「把阻塞检测放进
//! 独立线程 + `recv_timeout`」，不参与检测逻辑本身。
//!
//! # 能力边界（重要）
//!
//! 线程超时**只能放弃等待，不能终止执行体**：Rust 没有线程取消，超时后的线程会
//! detach 并继续持有驱动 / loader 锁。因此：
//!
//! - **Windows 的完整检测**已由宿主层 `device_probe` 子进程隔离（超时可强杀）；
//! - 本线程模型保留给 **macOS / Linux**（卡片范围决策）以及**廉价探测**
//!   （启动缓存校验，见 [`probe_adapter_fingerprints_with_timeout`]）。

use std::time::{Duration, Instant};

use lumino_core::{tr, trf};

use super::check::{FallbackDiagnostics, check_gpu_support, enumerate};
use super::report::{GpuCheckFailure, GpuCheckReport, required_backends};

/// 在独立线程中执行检测并施加超时。超时/线程异常均返回失败报告，不阻塞调用方。
///
/// `when` 透传到检测线程，语义同 [`check_gpu_support`]。
///
/// # 已知边界
///
/// 超时返回的失败报告只代表「没等到结果」，**不代表线程已停止**。宿主层在
/// Windows 上改用子进程（可强杀），本函数仅用于 macOS / Linux。
pub fn run_check_with_timeout(timeout: Duration, when: FallbackDiagnostics) -> GpuCheckReport {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawn_result = std::thread::Builder::new()
        .name("lumino-gpu-check".to_string())
        .spawn(move || {
            let report = check_gpu_support(when);
            let _ = tx.send(report);
        });
    if let Err(e) = spawn_result {
        tracing::error!("GPU 检测线程创建失败: {e}");
        return GpuCheckReport::failed(GpuCheckFailure::Aborted(trf!(
            "检测线程创建失败: {e}",
            e = e
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
            GpuCheckReport::failed(GpuCheckFailure::Aborted(
                tr!("检测线程意外退出").to_string(),
            ))
        }
    }
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
/// // cache_hit == false → 调用方执行 run_check_with_timeout(DEFAULT_TIMEOUT, FallbackDiagnostics::OnFailure) 全量检测
/// ```
///
/// # 最坏耗时
///
/// 最多阻塞 `timeout`（默认 [`crate::device_check::PROBE_TIMEOUT`] = 1.5 秒），
/// 取代修复前的「无限冻结」；配合全量检测 3 秒超时，启动最坏阻塞 ≈ 4.5 秒。
///
/// # 已知遗留（平台相关，记录在案）
///
/// - **Windows**：完整检测已由宿主层 `device_probe` 子进程隔离（可强杀），
///   但本廉价探测仍在父进程内跑线程模型——超时后探测线程 detach、无法回收。
///   后续卡：把探测也做成探针子进程的第二种模式，届时本包装可删除；
/// - **macOS / Linux**：全量检测同样沿用线程模型，超时后 detach 线程仍可能持有
///   驱动锁；当前以阶段一「结果区分 + 冷却 + 会话锁定」兜底。
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
