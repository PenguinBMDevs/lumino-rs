//! Runner 窗口事件处理（事件分派器）
//!
//! 大粒度的事件处理器已拆分到同级子模块，保持本文件「薄而清晰」：
//! - `dialog`：对话框类事件
//! - `audio_export` / `video_export_handler`：音频/视频导出事件
//! - `collaboration` / `sync`：协作与本地同步事件
//!
//! 共享的辅助方法（`open_dialog_traced` / `close_dialog_traced`）与
//! 预览帧缩放工具（`downscale_rgba`）保留在本文件中。

use std::time::Instant;

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_ui::event::window::{Event as WindowEvent, GpuCheckOutcome};

mod audio_export;
mod collaboration;
mod dialog;
mod sync;
mod track;
mod video_export;
mod video_export_handler;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Dialog(e) => self.handle_dialog_events(*e),
            WindowEvent::Collaboration(e) => self.handle_collaboration_events(e),
            WindowEvent::Sync(e) => self.handle_sync_events(e),
            WindowEvent::Track(e) => self.handle_track_events(e),
            WindowEvent::GpuCheckRun => self.on_gpu_check_run_requested(),
            WindowEvent::GpuCheckFinished {
                outcome,
                detail,
                fingerprint,
            } => self.on_gpu_check_finished(outcome, detail, fingerprint),
            WindowEvent::MidiPortLayoutChanged { max_port } => {
                // 端口编辑改变文档 max_port：按新布局重建实时合成输出，
                // 否则新端口事件会被旧通道空间静默丢弃。
                // DEBT-05 #122：走后台防抖重建（150ms 合并），UI 线程不做重活。
                tracing::info!("MIDI: 文档端口布局变更，请求后台重建 max_port={max_port}");
                self.midi_state
                    .midi
                    .apply_midi_port_layout_deferred(max_port);
            }
            WindowEvent::Lifecycle(e) => {
                // DEBT-05 #122：Lifecycle 变体已迁移到 take_window_action /
                // TrafficAction 路径，生产无发射方；显式告警而非静默吞掉——
                // 未来新增变体也会在穷尽匹配处直接暴露（本 match 无 `_` 兜底）。
                tracing::warn!("收到未消费的窗口生命周期事件（已迁移到 TrafficAction）: {e:?}");
            }
        }
    }

    /// 在后台线程执行 GPU 兼容性检查，完成后经事件总线回传纯文本结果 + 结论分类
    ///
    /// 检查耗时可达数百毫秒且可能触发驱动路径，必须在阻塞线程池执行，
    /// 不阻塞 UI/事件循环；UI 侧消费 `(outcome, detail)`，`fingerprint` 供 Runner 回写缓存。
    /// 手动检测人看得见结果（设置页展示 + 复制诊断），因此**始终**收集回退后端诊断。
    fn spawn_gpu_compatibility_check() {
        tracing::info!("兼容性页请求手动 GPU 兼容性检查");
        tokio::task::spawn_blocking(|| {
            let report = lumino_gfx::device_check::run_check_with_timeout(
                lumino_gfx::device_check::DEFAULT_TIMEOUT,
                lumino_gfx::device_check::FallbackDiagnostics::Always,
            );
            let outcome = crate::runner::gpu_check::outcome_of(&report);
            let detail = report.detail();
            let fingerprint = report.fingerprint.clone();
            lumino_ui::event::emit(lumino_ui::event::Event::Window(
                lumino_ui::event::window::Event::gpu_check_finished(outcome, detail, fingerprint),
            ));
        });
    }

    /// 把手动检测结果回写到启动缓存（与启动门控共用 `Storage` 实例，见 `device_gate`）。
    ///
    /// 语义与启动门控一致：「最近一次检测结果」——通过则下次启动走指纹缓存命中，
    /// 失败则下次启动重新全量检测（`cache_hit` 要求上次通过）。
    ///
    /// 回写失败只记 warn：磁盘异常不应影响用户已看到的检测结论。
    fn persist_manual_gpu_check_cache(&mut self, fingerprint: Option<&str>, passed: bool) {
        match self
            .window_state
            .storage
            .persist_gpu_check_cache(fingerprint, passed)
        {
            Ok(()) => tracing::info!(
                "手动 GPU 兼容性检测结果已写入缓存: passed={passed}, fingerprint={fingerprint:?}"
            ),
            Err(e) => tracing::warn!("手动 GPU 检测结果写入缓存失败（不影响结果展示）: {e}"),
        }
    }

    /// 手动检测请求：由 Runner 单点裁决（在飞 / 冷却 / 会话锁定一律拒绝）。
    ///
    /// 拒绝后仍回推一次权威状态：按钮可用性由 Runner 决定，设置面板不再本地置
    /// `Running`（否则关窗 / 开窗会让面板状态与真源分叉，形成二次点击缺口）。
    fn on_gpu_check_run_requested(&mut self) {
        if !self.gpu_check.try_begin(Instant::now()) {
            tracing::warn!("手动 GPU 检测请求被忽略：检测进行中 / 冷却中 / 会话已锁定");
            self.sync_gpu_check_ui(true);
            return;
        }
        self.sync_gpu_check_ui(true);
        Self::spawn_gpu_compatibility_check();
    }

    /// 检测结果落地：先回写缓存，再推进状态机并刷新 UI。
    ///
    /// 先落盘再注入 UI：设置对话框可能尚未就绪（分帧初始化），
    /// 但检测结果本身已经确定，缓存不应等待 UI。
    fn on_gpu_check_finished(
        &mut self,
        outcome: GpuCheckOutcome,
        detail: String,
        fingerprint: Option<String>,
    ) {
        self.persist_manual_gpu_check_cache(fingerprint.as_deref(), outcome.is_passed());
        self.gpu_check.finish(outcome, detail, Instant::now());
        self.sync_gpu_check_ui(true);
    }

    /// 逐帧维护 GPU 检测 UI：冷却到点推进 + 设置对话框（重）建时灌注一次。
    ///
    /// 必须在 `handle_control_flow` 之前调用：`tick` 清掉到点的冷却，
    /// 控制流计算才不会继续用过期 deadline 唤醒事件循环。
    pub(crate) fn about_to_wait_gpu_check(&mut self) {
        let now = Instant::now();
        if self.gpu_check.tick(now) {
            tracing::info!("GPU 检测冷却结束，按钮恢复可用");
            self.sync_gpu_check_ui(true);
            return;
        }
        self.sync_gpu_check_ui(false);
    }

    /// 把 Runner 侧的权威状态投影到设置对话框。
    ///
    /// `force=false` 仅在「设置对话框窗口刚创建 / 重开」时灌注一次（按窗口 id 去重），
    /// 避免每帧无意义地置脏；`force=true` 用于状态刚变化后的立即刷新。
    fn sync_gpu_check_ui(&mut self, force: bool) {
        let current = self
            .window_state
            .dialog_manager
            .first_dialog_id_of_type(DialogType::Settings);
        let Some(window_id) = current else {
            // 设置窗已关闭：清空记录，下次打开时重新灌注（含 Running / 冷却）
            self.gpu_check_ui_window = None;
            return;
        };
        if !force && self.gpu_check_ui_window == Some(window_id) {
            return;
        }
        let now = Instant::now();
        let state = self.gpu_check.ui_state();
        let gate = self.gpu_check.retry_gate(now);
        let applied = self
            .window_state
            .dialog_manager
            .apply_gpu_check_state(state.clone(), gate);
        // 注入失败（对话框 UI 尚未就绪）时不记录 id，下一帧继续重试
        self.gpu_check_ui_window = applied.then_some(window_id);
        tracing::debug!("GPU 检测状态已注入设置面板: {state:?} / 重试门控 {gate:?}");
    }

    /// 打开对话框并记录日志（保持「先日志后打开」顺序）。
    fn open_dialog_traced(&mut self, dialog: DialogType, label: &str) {
        tracing::info!("请求打开{label}对话框");
        self.window_state.dialog_manager.open_dialog(dialog);
    }

    /// 关闭对话框并记录日志（保持「先关闭后日志」顺序）。
    fn close_dialog_traced(&mut self, dialog: DialogType, label: &str) {
        self.window_state
            .dialog_manager
            .mark_dialog_for_close(dialog);
        tracing::info!("请求关闭{label}对话框");
    }
}

/// 最近邻缩放 + BGRA→RGBA 交换（零拷贝预览路径）。
///
/// 输入为 BGRA 格式，输出为 RGBA 格式。
/// 等价于先 `downscale_rgba` 再 `swap(0,2)`，但只需一次内存分配
/// 且避免全帧 clone（约 8MB@1080p）。
pub(super) fn downscale_bgra_to_rgba(
    src: &[u8],
    sw: u32,
    sh: u32,
    tw: u32,
    th: u32,
) -> (Vec<u8>, u32, u32) {
    if tw >= sw || th >= sh || tw == 0 || th == 0 {
        // 不需缩小：直接 clone 并在 clone 中交换
        let mut dst = src.to_vec();
        for pixel in dst.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        return (dst, sw, sh);
    }
    let mut dst = vec![0u8; (tw * th * 4) as usize];
    for dy in 0..th {
        let sy = (dy as f64 * sh as f64 / th as f64) as u32;
        let src_row = (sy * sw * 4) as usize;
        let dst_row = (dy * tw * 4) as usize;
        for dx in 0..tw {
            let sx = (dx as f64 * sw as f64 / tw as f64) as u32;
            let si = src_row + (sx * 4) as usize;
            let di = dst_row + (dx * 4) as usize;
            // BGRA → RGBA: 读取 B,G,R,A → 写入 R,G,B,A
            dst[di] = src[si + 2]; // R
            dst[di + 1] = src[si + 1]; // G
            dst[di + 2] = src[si]; // B
            dst[di + 3] = src[si + 3]; // A
        }
    }
    (dst, tw, th)
}
