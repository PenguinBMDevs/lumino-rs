//! 编辑器操作 - MIDI 输出管理

use crate::root::Root;

impl Root {
    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        tracing::debug!(
            "Root::load_tempo_changes: loading {} tempo changes",
            tempo_changes.len()
        );

        let tempo_change_list: Vec<crate::playback::TempoChange> = tempo_changes
            .into_iter()
            .map(|(tick, tempo)| crate::playback::TempoChange {
                tick: tick as f32,
                tempo,
            })
            .collect();

        // 如果有播放管理器，更新其 tempo timeline
        if let Some(manager) = &mut self.playback.manager {
            manager.update_tempo_changes(tempo_change_list);
            tracing::debug!("Root::load_tempo_changes: tempo changes updated in playback manager");
        } else {
            self.playback.pending_tempo_changes = Some(tempo_change_list);
            tracing::debug!(
                "Root::load_tempo_changes: playback manager not ready, cached tempo changes"
            );
        }
    }

    /// 设置 MIDI 输出连接
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi_io::OutputConnection>) {
        if let Some(manager) = &mut self.playback.manager {
            manager.set_midi_output(output);
            tracing::info!("Root::set_midi_output: MIDI output connection set");
        } else {
            self.playback.pending_midi_output = Some(output);
            tracing::debug!(
                "Root::set_midi_output: playback manager not ready, cached MIDI output"
            );
        }
    }

    /// 清除 MIDI 输出连接
    pub fn clear_midi_output(&mut self) {
        if let Some(manager) = &mut self.playback.manager {
            manager.clear_midi_output();
            tracing::info!("Root::clear_midi_output: MIDI output connection cleared");
        }
        self.playback.pending_midi_output = None;
    }
}

#[cfg(test)]
mod tests;
