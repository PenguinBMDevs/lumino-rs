//! 音频动作处理
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

/// 处理音频动作
pub fn handle_audio_action(
    output: &mut Box<dyn lumino_midi_io::OutputConnection>,
    action: lumino_ui::message::AudioAction,
) {
    use lumino_ui::message::AudioAction;

    match action {
        AudioAction::PlayNote { key, velocity } => {
            tracing::debug!("Runner: 调用 output.note_on(0, {}, {})", key, velocity);
            if let Err(e) = output.note_on(0, key, velocity) {
                tracing::warn!("播放音符失败: {}", e);
            }
        }
        AudioAction::StopNote { key } => {
            tracing::debug!("Runner: 调用 output.note_off(0, {}, 0)", key);
            if let Err(e) = output.note_off(0, key, 0) {
                tracing::warn!("停止音符失败: {}", e);
            }
        }
    }
}
