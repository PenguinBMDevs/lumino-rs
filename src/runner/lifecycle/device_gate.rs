//! 启动设备检查门控。
//!
//! 在 `resumed` 初始化主窗口之前执行：
//! 1. 只读加载配置（`Storage::new` 不写盘）
//! 2. 策略判定（是否检查 / 失败是否弹窗）
//! 3. 指纹缓存命中（TTL 内 + 指纹一致）时仅廉价探测，否则完整无头检测（≤3s 超时）
//! 4. 失败：按策略弹警告窗（等待用户）或静默继续（日志 + 状态栏提示）

use lumino_gfx::device_check::{self, GpuCheckFailure, GpuCheckReport};
use winit::event_loop::ActiveEventLoop;

use crate::runner::device_check_policy;
use crate::runner::device_warning::DeviceWarningWindow;
use crate::runner::inner::Runner;
use crate::storage;

/// 设备检查门控结果
pub(crate) enum DeviceGate {
    /// 继续正常初始化
    Proceed,
    /// 已创建警告窗，等待用户选择（初始化暂缓）
    Waiting,
    /// 用户选择退出
    Quit,
    /// 存储初始化失败
    StorageError(std::io::Error),
}

impl Runner {
    /// 执行启动设备检查门控（测试模式 / 调试开关由调用方跳过）
    pub(crate) fn run_device_gate(&mut self, event_loop: &ActiveEventLoop) -> DeviceGate {
        let mut storage = match storage::Storage::new() {
            Ok(storage) => storage,
            Err(e) => return DeviceGate::StorageError(e),
        };
        if storage.config.is_legacy_gpu_config() {
            tracing::info!("检测到旧版配置（缺 GPU 字段）：归一化为静默模式（设置页可恢复警告）");
        }

        let policy = device_check_policy::resolve_policy(&storage.config.get().ui);
        if !policy.run_check {
            tracing::info!("设备检查已在设置中关闭，跳过");
            self.pending_storage = Some(storage);
            return DeviceGate::Proceed;
        }

        let config = storage.config.get();
        let cached_fp = config.ui.gpu_last_fingerprint.clone();
        let cached_passed = config.ui.gpu_last_passed;
        let cached_time = config.ui.gpu_last_check_time;
        let ui_snapshot = config.ui.clone();

        // 缓存时效：上次完整检测的时间戳必须在 TTL 内。
        // macOS（Metal）的适配器指纹恒定（driver / driver_info 为空串），只有时间兜底才能让
        // 系统 / 驱动大版本升级后重新检测；旧配置缺 gpu_last_check_time、系统时钟异常、
        // 调试开关强制过期 → 一律按 miss 处理（宁可多检一次，不漏检）。
        let within_ttl = if device_check_policy::debug_expire_cache_requested() {
            tracing::warn!("LUMINO_DEBUG_STALE_GPU_CACHE 已设置：强制按缓存过期处理，执行全量检测");
            false
        } else if device_check_policy::cache_within_ttl(cached_time, storage::now_unix_secs()) {
            true
        } else {
            tracing::info!(
                "GPU 检测缓存已过期或缺失（上次检测时间戳 {:?}，TTL {} 天），执行全量检测",
                cached_time,
                device_check_policy::GPU_CHECK_CACHE_TTL_DAYS
            );
            false
        };

        // 指纹缓存命中（TTL 内 + 上次通过 + 适配器指纹仍在）：廉价探测后直接放行。
        // 调试强制失败时不走缓存，保证 LUMINO_FORCE_DEVICE_CHECK_FAIL 可复现；
        // TTL 过期时不执行廉价探测，直接落回全量检测。
        // 廉价探测也走独立线程 + 超时（UI-008 / #37）：主线程绝不允许直接探测。
        // 超时/异常由 `unwrap_or_default()` 折算为空指纹列表 → cache miss → 全量检测；
        // 探测开始/完成/超时日志由 `probe_adapter_fingerprints_with_timeout` 统一打印。
        let cache_hit = device_check_policy::cache_hit(
            within_ttl,
            device_check::debug_force_fail_requested(),
            cached_passed,
            cached_fp.as_deref(),
            || {
                device_check::probe_adapter_fingerprints_with_timeout(device_check::PROBE_TIMEOUT)
                    .unwrap_or_default()
            },
        );

        let report = if cache_hit {
            tracing::info!(
                "GPU 适配器指纹未变化，跳过完整试画检测（缓存命中，上次检测时间戳 {:?}）",
                cached_time
            );
            GpuCheckReport {
                passed: true,
                failure: None,
                required_backend: device_check::required_backend_name(),
                adapters: Vec::new(),
                fallback_adapters: Vec::new(),
                fingerprint: cached_fp.clone(),
                duration_ms: 0,
            }
        } else {
            tracing::info!("GPU 适配器指纹缓存未命中，执行全量兼容性检测");
            device_check::run_check_with_timeout(device_check::DEFAULT_TIMEOUT)
        };

        let fingerprint = report.fingerprint.clone();
        let passed = report.passed;
        // 结果有变化，或本次未走缓存（TTL 过期 / 旧配置无时间戳 / 调试开关）→ 需要回写。
        // 注意 `!within_ttl`：TTL 过期触发的全量检测必须刷新时间戳，否则此后每次启动
        // 都会被判定为过期，退化成"每次启动都全量检测"。
        let cache_changed =
            !within_ttl || cached_fp != fingerprint || cached_passed != Some(passed);

        if passed {
            if cache_changed
                && let Err(e) = storage.persist_gpu_check_cache(fingerprint.as_deref(), passed)
            {
                tracing::warn!("写入 GPU 检测缓存失败: {e}");
            }
            self.pending_storage = Some(storage);
            return DeviceGate::Proceed;
        }

        let detail = failure_detail(&report);

        // 抑制弹窗：静默继续（写日志 + 状态栏提示）
        if !policy.show_warning {
            tracing::warn!(
                "GPU 兼容性检测失败（已抑制弹窗，静默继续）: {}",
                report.detail().replace('\n', " | ")
            );
            self.pending_status_hint = Some(format!(
                "未检测到 {} 支持，运行可能不稳定（可在设置中查看详情）",
                device_check::required_backend_name()
            ));
            if let Err(e) = storage.persist_gpu_check_cache(fingerprint.as_deref(), passed) {
                tracing::warn!("写入 GPU 检测缓存失败: {e}");
            }
            self.pending_storage = Some(storage);
            return DeviceGate::Proceed;
        }

        // 弹警告窗等待用户选择
        match DeviceWarningWindow::create(event_loop, &ui_snapshot, &detail) {
            Ok(window) => {
                tracing::warn!("GPU 兼容性检测失败，已弹出警告窗等待用户选择");
                self.device_warning = Some(window);
                self.pending_gpu_cache = Some((fingerprint, false));
                self.pending_storage = Some(storage);
                DeviceGate::Waiting
            }
            Err(e) => {
                tracing::error!("警告窗创建失败（{e}），回退原生提示框");
                if show_native_warning(&detail) {
                    if let Err(e) = storage.persist_gpu_check_cache(fingerprint.as_deref(), passed)
                    {
                        tracing::warn!("写入 GPU 检测缓存失败: {e}");
                    }
                    self.pending_storage = Some(storage);
                    DeviceGate::Proceed
                } else {
                    DeviceGate::Quit
                }
            }
        }
    }

    /// 处理警告窗阶段的用户选择（主窗口尚未初始化时由 `about_to_wait` 调用）
    pub(crate) fn about_to_wait_device_warning(&mut self, event_loop: &ActiveEventLoop) {
        if self.device_warning.is_none() {
            return;
        }
        let action = self.device_warning.as_mut().and_then(|w| w.take_action());
        let Some(action) = action else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            return;
        };

        match action {
            lumino_ui::window::DeviceWarningAction::Quit => {
                tracing::info!("设备检查警告窗：用户选择「确认并关闭」");
                self.device_warning = None;
                event_loop.exit();
            }
            lumino_ui::window::DeviceWarningAction::Continue { suppress } => {
                tracing::info!("设备检查警告窗：用户选择「放我进去」（不再提示={suppress}）");
                if let Some(window) = self.device_warning.as_mut() {
                    window.close();
                }
                self.device_warning = None;

                let (fingerprint, passed) = self.pending_gpu_cache.clone().unwrap_or((None, false));
                if let Some(storage) = self.pending_storage.as_mut() {
                    if suppress && let Err(e) = storage.set_gpu_warning_suppressed(true) {
                        tracing::warn!("写入 GPU 警告抑制状态失败: {e}");
                    }
                    if let Err(e) = storage.persist_gpu_check_cache(fingerprint.as_deref(), passed)
                    {
                        tracing::warn!("写入 GPU 检测缓存失败: {e}");
                    }
                }
                self.pending_gpu_cache = None;

                match self.init_inner(event_loop) {
                    Ok(inner) => {
                        self.inner = Some(inner);
                        self.after_inner_started();
                    }
                    Err(e) => {
                        tracing::error!("Runner 初始化失败：{e}");
                        self.init_error = Some(e);
                        event_loop.exit();
                    }
                }
            }
        }
    }

    /// 主窗口初始化完成后的收尾（`resumed` 与警告窗「继续」共用）
    pub(crate) fn after_inner_started(&mut self) {
        // 兜底首帧：显式请求一次主窗口重绘。
        // Wait 模式下 about_to_wait 不再每轮强制 request_redraw，
        // 首帧后若无动画/播放，needs_redraw==false 即进入休眠，不会忙循环。
        if let Some(this) = self.inner.as_mut() {
            this.window_state.window.request_redraw();
        }

        // 静默失败路径：写入状态栏提示
        if let Some(hint) = self.pending_status_hint.take()
            && let Some(this) = self.inner.as_mut()
        {
            this.window_state
                .window
                .ui_mut()
                .set_status_message(Some(hint));
        }

        // 启动自动连接云存储（后台静默执行，失败不打扰用户）
        if let Some(this) = self.inner.as_mut() {
            this.startup_auto_connect();
        }
    }
}

/// 原生提示框兜底（iced 警告窗创建失败时使用）；返回是否选择继续启动
fn show_native_warning(detail: &str) -> bool {
    const CONTINUE_LABEL: &str = "放我进去";
    const QUIT_LABEL: &str = "确认并关闭";
    matches!(
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("GPU 兼容性检查")
            .set_description(detail)
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                CONTINUE_LABEL.to_string(),
                QUIT_LABEL.to_string(),
            ))
            .show(),
        rfd::MessageDialogResult::Custom(label) if label == CONTINUE_LABEL
    )
}

/// 失败详情文案（弹窗"详情"行；按失败分类给出可操作建议，并附回退后端提示）
fn failure_detail(report: &GpuCheckReport) -> String {
    let backend = device_check::required_backend_name();
    let base = match &report.failure {
        Some(GpuCheckFailure::NoAdapter) => {
            format!("未检测到支持 {backend} 的显卡或驱动，可尝试更新显卡驱动。")
        }
        Some(GpuCheckFailure::DeviceRequest(_)) => {
            format!("检测到 {backend} 设备，但初始化失败（驱动可能不满足要求）。")
        }
        Some(GpuCheckFailure::RenderError(_)) => {
            format!("{backend} 渲染管线执行失败，请尝试更新显卡驱动。")
        }
        Some(GpuCheckFailure::Timeout) => {
            format!("{backend} 检测超时，驱动可能异常；已跳过本次检测。")
        }
        None => format!("{backend} 兼容性检测未通过。"),
    };

    if report.fallback_adapters.is_empty() {
        return base;
    }
    let mut names: Vec<&str> = report
        .fallback_adapters
        .iter()
        .map(|adapter| adapter.backend.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    format!(
        "{base}（检测到其他可用后端：{}，可能仍可运行）",
        names.join("/")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_gfx::device_check::GpuAdapterSummary;

    #[test]
    fn test_failure_detail_mentions_required_backend() {
        let report = GpuCheckReport::failed(GpuCheckFailure::NoAdapter);
        let detail = failure_detail(&report);
        assert!(detail.contains(device_check::required_backend_name()));
    }

    #[test]
    fn test_failure_detail_appends_fallback_backends() {
        let mut report = GpuCheckReport::failed(GpuCheckFailure::NoAdapter);
        report.fallback_adapters = vec![
            GpuAdapterSummary {
                name: "gpu".into(),
                backend: "Dx12".into(),
                device_type: "DiscreteGpu".into(),
                driver: "d".into(),
                driver_info: "i".into(),
            },
            GpuAdapterSummary {
                name: "gpu2".into(),
                backend: "Dx12".into(),
                device_type: "IntegratedGpu".into(),
                driver: "d".into(),
                driver_info: "i".into(),
            },
        ];
        let detail = failure_detail(&report);
        assert!(detail.contains("Dx12"));
    }

    #[test]
    fn test_failure_detail_timeout_message() {
        let report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
        assert!(failure_detail(&report).contains("超时"));
    }
}
