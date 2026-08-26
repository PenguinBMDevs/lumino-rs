//! MIDI 重初始化模块

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理 MIDI 重新初始化和 XSynth 异步初始化检查
    pub(crate) fn handle_midi_reinit(&mut self) {
        // 检查音频流恢复（音频设备被拔出/更换后自动重定向/重建）
        self.midi_state.midi.handle_stream_recovery();

        // 检查是否需要重新初始化 MIDI
        if self.midi_state.midi.needs_reinit() {
            let ui_config = self.window_state.storage.config.get().ui.clone();
            self.midi_state.midi.reinit_if_needed(&ui_config);

            // 同步后端（Core / System / Kdmapi）在 reinit 后立即就绪，
            // 必须立即把播放引擎的 MIDI 输出重连到新连接，否则 PlaybackManager
            // 仍指向已被丢弃的旧连接 → 切换后无声 / 无响应。
            // XSynth-Realtime / LGS (GPU) 走异步初始化路径，待
            // check_async_init_complete 完成时再重连。
            if !self.midi_state.midi.is_xsynth_initializing()
                && !self.midi_state.midi.is_lgs_initializing()
            {
                Self::reconnect_playback_output(self);
            }
        }

        // 检查 XSynth 异步初始化是否完成
        if self.midi_state.midi.check_async_init_complete() {
            tracing::info!("XSynth: 异步初始化完成，正在创建新的播放连接...");
            Self::reconnect_playback_output(self);
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
                tracing::info!("MIDI: 播放输出已重连到当前后端");
            }
            None => tracing::error!("MIDI: 无法创建播放输出，播放将无声"),
        }
    }
}
