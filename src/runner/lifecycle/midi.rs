//! MIDI 重初始化模块

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理 MIDI 重新初始化和 XSynth 异步初始化检查
    pub(crate) fn handle_midi_reinit(&mut self) {
        // 检查是否需要重新初始化 MIDI
        if self.midi_state.midi.needs_reinit() {
            let ui_config = self.window_state.storage.config.get().ui.clone();
            self.midi_state.midi.reinit_if_needed(&ui_config);
        }

        // 检查 XSynth 异步初始化是否完成
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
