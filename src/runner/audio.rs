use super::RunnerInner;

impl RunnerInner {
    pub(super) fn process_audio_actions(&mut self) {
        let actions = self.ui.take_audio_actions();

        for action in actions {
            self.handle_audio_action(action);
        }
    }

    fn handle_audio_action(&mut self, action: lumino_ui::message::AudioAction) {
        use lumino_ui::message::AudioAction;

        let Some(output) = &mut self.midi_output else {
            return; // 没有 MIDI 输出设备
        };

        match action {
            AudioAction::PlayNote { key, velocity } => {
                // 在 MIDI 通道 0 上播放音符
                if let Err(e) = output.note_on(0, key, velocity) {
                    tracing::warn!("播放音符失败: {}", e);
                }
            }
            AudioAction::StopNote { key } => {
                // 在 MIDI 通道 0 上停止音符
                if let Err(e) = output.note_off(0, key, 0) {
                    tracing::warn!("停止音符失败: {}", e);
                }
            }
        }
    }
}
