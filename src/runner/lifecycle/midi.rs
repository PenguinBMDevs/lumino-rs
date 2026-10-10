//! MIDI 重初始化模块

use std::time::{Duration, Instant};

use crate::runner::inner::RunnerInner;
use crate::runner::midi_manager::AsyncInitOutcome;

/// #127：播放输出创建失败后的自动重试次数。
const MIDI_OUTPUT_RETRY_ATTEMPTS: u8 = 3;
/// #127：播放输出创建失败后的自动重试间隔。
const MIDI_OUTPUT_RETRY_INTERVAL: Duration = Duration::from_millis(750);

/// #127：播放输出创建失败后的自愈重试状态。
///
/// 启动创建失败时由 [`MidiOutputRetry::new`] 立即武装；运行期重连失败由
/// [`RunnerInner::arm_midi_output_retry`] 武装；每帧由 `tick_midi_output_retry`
/// 检查到期并执行，次数耗尽后向用户给出可见提示。
#[derive(Default)]
pub(crate) struct MidiOutputRetry {
    /// 剩余重试次数（0 = 无待重试）
    left: u8,
    /// 下一次重试时间（None = 无待重试）
    at: Option<Instant>,
    /// 失败提示是否已写入状态栏（恢复后清除，避免残留）
    failure_notice: bool,
}

impl MidiOutputRetry {
    /// 武装重试（幂等：已向用户提示后不再自动武装）。
    fn arm(&mut self) {
        if self.failure_notice {
            return;
        }
        self.left = MIDI_OUTPUT_RETRY_ATTEMPTS;
        self.at = Some(Instant::now() + MIDI_OUTPUT_RETRY_INTERVAL);
        tracing::warn!(
            "MIDI: 播放输出不可用，将在 {}ms 后自动重试（最多 {} 次）",
            MIDI_OUTPUT_RETRY_INTERVAL.as_millis(),
            MIDI_OUTPUT_RETRY_ATTEMPTS
        );
    }

    /// 重试/重连成功：清空重试状态；返回此前是否已向用户提示（供清状态栏）。
    fn clear(&mut self) -> bool {
        self.left = 0;
        self.at = None;
        std::mem::take(&mut self.failure_notice)
    }

    /// 是否处于等待重试状态。
    fn waiting(&self) -> bool {
        self.at.is_some()
    }

    /// 是否已到重试时间。
    fn due(&self) -> bool {
        matches!(self.at, Some(at) if Instant::now() >= at)
    }

    /// 记录一次失败并安排下一次重试；返回 `false` 表示次数耗尽（已标记需提示）。
    fn schedule_next(&mut self) -> bool {
        self.left = self.left.saturating_sub(1);
        if self.left == 0 {
            self.at = None;
            self.failure_notice = true;
            return false;
        }
        self.at = Some(Instant::now() + MIDI_OUTPUT_RETRY_INTERVAL);
        true
    }
}

impl RunnerInner {
    /// 启动时为播放引擎创建独立输出连接（#127：失败转入自愈重试）。
    ///
    /// 这样用户自绘音符在点击播放按钮时能正常发声；创建失败时不再只记日志，
    /// 由自愈重试接管（每帧在 [`Self::handle_midi_reinit`] 中检查到期）。
    pub(crate) fn setup_playback_output(&mut self) {
        match self.midi_state.midi.create_additional_output() {
            Some(output) => {
                self.window_state
                    .window
                    .ui_mut()
                    .set_playback_midi_output(output);
                tracing::info!("Runner: 播放引擎 MIDI 输出连接已就绪");
            }
            None => {
                tracing::error!("Runner: 无法创建播放引擎 MIDI 输出，播放将无声");
                self.arm_midi_output_retry();
            }
        }
    }

    /// 处理 MIDI 重新初始化和异步初始化检查
    pub(crate) fn handle_midi_reinit(&mut self) {
        // DEBT-05 #122：先收尾后台端口布局重建（归还 API、必要追赶），
        // 再做流恢复/重初始化等需要 API 的动作。
        self.midi_state.midi.poll_layout_apply();

        // 检查音频流恢复（音频设备被拔出/更换后自动重定向/重建）
        self.midi_state.midi.handle_stream_recovery();

        // 检查是否需要重新初始化 MIDI
        if self.midi_state.midi.needs_reinit() {
            // #127 问题2：重初始化会重开 WinMM 主输出。先同步释放播放引擎持有的
            // 旧连接——同一进程内 WinMM 不允许对同一端口建立第二个连接，旧连接
            // 未释放时 `init_system_output` 必然失败，随后 `create_additional_output`
            // 三策略连锁失败，表现为换设备后永久无声。
            if !self
                .window_state
                .window
                .ui_mut()
                .clear_playback_midi_output_sync()
            {
                tracing::warn!(
                    "MIDI: 同步释放播放输出未获回执（播放线程超时），继续重建并依赖自愈重试"
                );
            }

            let ui_config = self.window_state.storage.config.get().ui.clone();
            self.midi_state.midi.reinit_if_needed(&ui_config);

            // 同步后端（Core / System / Kdmapi）在 reinit 后立即就绪，
            // 必须立即把播放引擎的 MIDI 输出重连到新连接，否则 PlaybackManager
            // 仍指向已被丢弃的旧连接 → 切换后无声 / 无响应。
            // XSynth-Realtime / LGS (GPU) 走异步初始化路径，待
            // check_async_init_complete 落定时再重连。
            if !self.midi_state.midi.is_xsynth_initializing()
                && !self.midi_state.midi.is_lgs_initializing()
            {
                Self::reconnect_playback_output(self);
            }
        }

        // 检查异步后端初始化是否落定（#127：成功与失败都必须重连——
        // 失败时同步兜底后端（System）的连接已在快速启动时建立，
        // 若不重连，播放引擎停留在已被释放的旧连接上 → 无声）。
        match self.midi_state.midi.check_async_init_complete() {
            AsyncInitOutcome::Switched => {
                tracing::info!("MIDI: 异步后端初始化完成，正在创建新的播放连接...");
                Self::reconnect_playback_output(self);
            }
            AsyncInitOutcome::Failed => {
                tracing::warn!("MIDI: 异步后端初始化失败，保持当前后端并重连播放输出");
                Self::reconnect_playback_output(self);
            }
            AsyncInitOutcome::Pending => {}
        }

        // #127：播放输出创建失败的自愈重试（换设备窗口期端口可能瞬时不可用）。
        self.tick_midi_output_retry();

        // #127 问题1：把后端降级/失败提示推送到状态栏（取走即清，避免重复提示）。
        if let Some(notice) = self.midi_state.midi.take_pending_backend_notice() {
            tracing::warn!("MIDI 后端提示: {notice}");
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(notice));
        }
    }

    /// 把当前已就绪的 MIDI 输出连接注入播放引擎，使播放输出现实音频。
    ///
    /// 切换后端 / 修改音频设置后必须调用：旧连接已被丢弃，若不重连，
    /// PlaybackManager 会继续向死连接发送事件，表现为「切换后无声 / 无响应」。
    fn reconnect_playback_output(&mut self) {
        match self.midi_state.midi.create_additional_output() {
            Some(output) => {
                self.window_state
                    .window
                    .ui_mut()
                    .set_playback_midi_output(output);
                if self.midi_output_retry.clear() {
                    self.window_state.window.ui_mut().set_status_message(None);
                }
                tracing::info!("MIDI: 播放输出已重连到当前后端");
            }
            None => {
                tracing::error!("MIDI: 无法创建播放输出，播放将无声");
                self.arm_midi_output_retry();
            }
        }
    }

    /// 武装播放输出自愈重试（幂等：已向用户提示后不再自动武装）。
    pub(crate) fn arm_midi_output_retry(&mut self) {
        self.midi_output_retry.arm();
    }

    /// 到期执行一次播放输出重试；重试耗尽后给出用户可见提示并停止自动重试。
    fn tick_midi_output_retry(&mut self) {
        if !self.midi_output_retry.waiting() || !self.midi_output_retry.due() {
            return;
        }

        match self.midi_state.midi.create_additional_output() {
            Some(output) => {
                self.window_state
                    .window
                    .ui_mut()
                    .set_playback_midi_output(output);
                if self.midi_output_retry.clear() {
                    self.window_state.window.ui_mut().set_status_message(None);
                }
                tracing::info!("MIDI: 播放输出自动重试成功");
            }
            None => {
                if self.midi_output_retry.schedule_next() {
                    tracing::warn!(
                        "MIDI: 播放输出重试失败，剩余 {} 次",
                        self.midi_output_retry.left
                    );
                } else {
                    self.window_state.window.ui_mut().set_status_message(Some(
                        "MIDI 播放输出创建失败，播放将无声：请检查输出设备（设置 → MIDI）后重试"
                            .to_string(),
                    ));
                    tracing::error!("MIDI: 播放输出自动重试耗尽，播放将无声（已提示用户）");
                }
            }
        }
    }
}
