use super::RunnerInner;

impl RunnerInner {
    pub(super) fn process_audio_actions(&mut self) {
        let actions = self.ui.take_audio_actions();

        if !actions.is_empty() {
            tracing::info!("Runner: 处理 {} 个音频动作", actions.len());
            for (i, action) in actions.iter().enumerate() {
                match action {
                    lumino_ui::message::AudioAction::PlayNote { key, velocity } => {
                        tracing::info!("  [{}] PlayNote: key={}, velocity={}", i, key, velocity);
                    }
                    lumino_ui::message::AudioAction::StopNote { key } => {
                        tracing::info!("  [{}] StopNote: key={}", i, key);
                    }
                }
            }
        }

        for action in actions {
            self.handle_audio_action(action);
        }
    }

    fn handle_audio_action(&mut self, action: lumino_ui::message::AudioAction) {
        use lumino_ui::message::AudioAction;

        let Some(output) = &mut self.midi_output else {
            tracing::warn!("MIDI 输出未初始化，无法播放音频");
            return;
        };

        // 打印类型信息
        tracing::debug!("output 类型: {}", std::any::type_name_of_val(&**output));

        match action {
            AudioAction::PlayNote { key, velocity } => {
                tracing::debug!("Runner: 调用 output.note_on(0, {}, {})", key, velocity);
                if let Err(e) = output.note_on(0, key, velocity) {
                    tracing::warn!("播放音符失败: {}", e);
                } else {
                    tracing::debug!("Runner: output.note_on 返回成功");
                }
            }
            AudioAction::StopNote { key } => {
                tracing::debug!("Runner: 调用 output.note_off(0, {}, 0)", key);
                if let Err(e) = output.note_off(0, key, 0) {
                    tracing::warn!("停止音符失败: {}", e);
                } else {
                    tracing::debug!("Runner: output.note_off 返回成功");
                }
            }
        }
    }
}
