//! MIDI 重初始化模块

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理 MIDI 重新初始化和 XSynth 异步初始化检查
    pub(crate) fn handle_midi_reinit(&mut self) {
        // 检查是否需要重新初始化 MIDI
        if self.midi_state.midi.needs_reinit() {
            // 【修复】重初始化前暂停播放，防止墙钟继续走导致位置不同步
            let was_playing = self.window_state.window.ui().is_playing();
            if was_playing {
                self.window_state.window.ui_mut().pause_playback();
            }

            let ui_config = self.window_state.storage.config.get().ui.clone();
            self.midi_state.midi.reinit_if_needed(&ui_config);

            // 【修复】重初始化后必须将新 MIDI 输出连接注入 PlaybackManager
            // 否则 PlaybackManager.midi_output 仍然挂在旧的死 AudioCommandAdapter 上，
            // MIDI 事件发送到已销毁的音频引擎的 cmd_tx，导致无声。
            if let Some(output) = self.midi_state.midi.create_additional_output() {
                self.window_state
                    .window
                    .ui_mut()
                    .set_playback_midi_output(output);
                tracing::info!("MIDI 重初始化后：播放引擎 MIDI 输出已更新");
            } else {
                tracing::error!("MIDI 重初始化后：无法创建新输出连接，播放将无声");
            }
        }

        // 检查 XSynth 异步初始化是否完成
        // 注意：XSynth 新引擎为同步初始化，此方法总是返回 false，
        // 但保留此逻辑以兼容旧后端（KDMAPI/System）或未来的异步初始化。
        if self.midi_state.midi.check_async_init_complete() {
            tracing::info!("XSynth: 异步初始化完成，正在创建新的播放连接...");
            if let Some(output) = self.midi_state.midi.create_additional_output() {
                self.window_state
                    .window
                    .ui_mut()
                    .set_playback_midi_output(output);
                tracing::info!("XSynth: 播放引擎 MIDI 输出已更新为 XSynth");
            } else {
                tracing::error!("XSynth: 无法创建 XSynth 播放输出，播放将无声");
            }
        }
    }
}
